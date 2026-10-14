//! brumm is an Apple Music and Spotify player for Omarchy.
//!
//! ```text
//! brumm          open the player as picked in its options: tui (default) or gui
//! brumm --tui    open the player in this terminal (starts the daemon if needed)
//! brumm --gui    open the player as a window
//! brumm open     open the player as picked, in a window or a new terminal (the launcher's entry)
//! brumm daemon   run the background daemon (usually via systemd)
//! brumm login    sign in to the music service in the browser, then open the player
//! brumm provider [apple|spotify]  show the music services, or switch to one
//! brumm update   install the newest release now and offer to restart into it
//! brumm keys on|off  SUPER+SHIFT+M (and +ALT) open brumm, or Omarchy's music apps
//! brumm setup D  install from an unpacked release D (used by install.sh)
//! ```

mod art;
mod chrome;
mod config;
mod daemon;
mod engine;
mod error;
mod gui;
mod httpd;
mod ipc;
mod launch;
mod login;
mod mpris;
mod music;
mod proc;
mod provider;
mod tui;
mod update;

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use config::{Options, VERSION};
use error::{Result, err};

fn runtime(workers: usize) -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .expect("tokio runtime")
}

fn terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    let arg = |i: usize| args.get(i).map(String::as_str);
    let r: Result<()> = match cmd {
        "" => {
            if launch::default() == launch::GUI {
                launch::open(launch::GUI) // the window, and the terminal back
            } else {
                tui::run(&runtime(2))
            }
        }
        "--tui" | "tui" => tui::run(&runtime(2)),
        "--gui" | "gui" => gui::run(&runtime(1)),
        "open" => {
            let ui = arg(2).unwrap_or(launch::default());
            if ui == launch::GUI {
                gui::run(&runtime(1))
            } else {
                launch::open(launch::TUI)
            }
        }
        "daemon" => runtime(4).block_on(daemon::run(VERSION)),
        "login" => login(),
        "provider" => provider(arg(2)),
        "setup" => {
            let from = arg(2).unwrap_or(".");
            update::install(Path::new(from))
                .map(|_| println!("installed brumm to {}", update::bin_path().display()))
        }
        "update" => self_update(),
        "keys" => match arg(2) {
            Some(v @ ("on" | "off")) => {
                let on = v == "on";
                update::set_music_keys(on).and_then(|_| {
                    let mut o = Options::load();
                    o.music_keys = on;
                    o.save()?;
                    println!(
                        "{}",
                        if on {
                            "SUPER+SHIFT+M opens brumm, SUPER+SHIFT+ALT+M brumm in the terminal"
                        } else {
                            "the music keys are Omarchy's again"
                        }
                    );
                    Ok(())
                })
            }
            _ => Err(err(
                "usage: brumm keys on|off — SUPER+SHIFT+M (and +ALT) open brumm, or Omarchy's music apps again",
            )),
        },
        "version" | "--version" => {
            println!("brumm {VERSION}");
            Ok(())
        }
        _ => {
            eprintln!(
                "usage: brumm [--tui | --gui | open [tui|gui] | keys on|off | provider [apple|spotify] | daemon | login | update | setup DIR | version]"
            );
            std::process::exit(2);
        }
    };
    if let Err(e) = r {
        eprintln!("brumm: {e}");
        std::process::exit(1);
    }
}

/// Signs in, tells a running daemon, and at a terminal goes straight to
/// the music. A first sign-in at a terminal asks for the service first.
fn login() -> Result<()> {
    let mut cfg = config::Config::load()?;
    if cfg.needs_provider_choice() && terminal() {
        cfg.provider = ask_provider()?;
        cfg.save()?;
    }
    let rt = runtime(2);
    rt.block_on(login::run(|s| println!("{s}")))?;
    println!("signed in");
    rt.block_on(async {
        if let Ok((c, _)) = ipc::Client::dial().await {
            let _ = c.request(ipc::Request::new(ipc::CMD_RELOAD)).await;
            c.close().await;
        }
    });
    if terminal() {
        return tui::run(&rt);
    }
    Ok(())
}

/// Asks at the terminal which music service to use; enter takes the first.
fn ask_provider() -> Result<String> {
    let choices: Vec<_> = provider::registry::choices()
        .into_iter()
        .filter(|p| p.available)
        .collect();
    println!("Which music service do you use?");
    for (i, p) in choices.iter().enumerate() {
        println!("  {}  {}", i + 1, p.name);
    }
    loop {
        print!("[1-{}] ", choices.len());
        std::io::stdout().flush()?;
        let mut answer = String::new();
        if std::io::stdin().lock().read_line(&mut answer)? == 0 {
            return Err(err("no service chosen"));
        }
        let answer = answer.trim();
        let i = if answer.is_empty() {
            Some(0)
        } else {
            answer.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
        };
        if let Some(p) = i.and_then(|i| choices.get(i)) {
            return Ok(p.id.clone());
        }
    }
}

/// Lists the music services, or switches to one: a running daemon switches
/// at once; otherwise the next start uses it.
fn provider(id: Option<&str>) -> Result<()> {
    use provider::registry;
    let Some(id) = id else {
        let sel = registry::selection(&config::Config::load()?);
        for p in sel.choices {
            println!(
                "{} {:<8} {}{}",
                if p.id == sel.selected { "*" } else { " " },
                p.id,
                p.name,
                if p.available { "" } else { " (coming soon)" }
            );
        }
        return Ok(());
    };
    registry::validate(id).map_err(|e| match e {
        error::Error::Unsupported => err(format!("{} is not available yet", registry::name(id))),
        e => e,
    })?;
    let rt = runtime(1);
    let told = rt.block_on(async {
        let (c, _) = ipc::Client::dial().await.ok()?;
        let r = c
            .request(ipc::Request {
                cmd: ipc::CMD_PROVIDER.into(),
                query: id.into(),
                ..Default::default()
            })
            .await;
        c.close().await;
        Some(r)
    });
    match told {
        Some(r) => {
            r?;
        }
        None => {
            let mut cfg = config::Config::load()?;
            cfg.provider = id.into();
            cfg.save()?;
        }
    }
    let cfg = config::Config::load()?;
    println!("brumm plays from {}", cfg.service_name());
    if cfg.signed_in() == Ok(false) {
        println!("sign in with: brumm login");
    }
    Ok(())
}

fn self_update() -> Result<()> {
    if !update::managed(VERSION) {
        return Err(err(format!(
            "this brumm ({VERSION}) was not installed from a release; use bin/update in the source checkout"
        )));
    }
    let rt = runtime(2);
    let (tag, installed) = rt.block_on(update::update(VERSION))?;
    if installed {
        println!("updated to {tag}");
        rt.block_on(restart_offer());
        return Ok(());
    }
    println!("brumm {VERSION} is current");
    if rt.block_on(update::restore_window(VERSION))? {
        println!("restored the window (brumm --gui)");
    }
    Ok(())
}

/// Asks, at a terminal, to restart the running player into the update now;
/// it plays on where it was. Otherwise it waits for a pause.
async fn restart_offer() {
    let later = "brumm restarts into it when playback is paused or a song ends";
    let Ok((c, _)) = ipc::Client::dial().await else {
        return;
    }; // nothing running: the next start is the new one
    if !terminal() {
        println!("{later}");
        return;
    }
    print!("restart now? the music goes on where it is [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().lock().read_line(&mut answer);
    let a = answer.trim().to_lowercase();
    if !matches!(a.as_str(), "" | "y" | "yes" | "j" | "ja") {
        println!("{later}");
        return;
    }
    match c
        .request(ipc::Request {
            cmd: ipc::CMD_UPDATE.into(),
            value: 3.0,
            ..Default::default()
        })
        .await
    {
        Ok(_) => println!("restarting"),
        Err(e) => println!("could not restart: {e} — {later}"),
    }
    c.close().await;
}
