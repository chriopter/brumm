//! brumm is an Apple Music player for Omarchy.
//!
//! ```text
//! brumm          open the player as picked in its options: tui (default) or gui
//! brumm --tui    open the player in this terminal (starts the daemon if needed)
//! brumm --gui    open the player as a window
//! brumm open     open the player as picked, in a window or a new terminal (the launcher's entry)
//! brumm daemon   run the background daemon (usually via systemd)
//! brumm login    sign in to Apple Music in the browser, then open the player
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
                "usage: brumm [--tui | --gui | open [tui|gui] | keys on|off | daemon | login | update | setup DIR | version]"
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
/// the music.
fn login() -> Result<()> {
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
