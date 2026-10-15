//! Starts brumm's pieces: the daemon behind the player, and the player
//! itself as the terminal UI or the window (GUI).

use std::fs;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::config::{self, Options};
use crate::error::{Result, err};
use crate::ipc::{Client, Message};
use crate::proc;
use crate::update::which;

// The two faces of the player.
pub const TUI: &str = "tui";
pub const GUI: &str = "gui";

/// The face plain `brumm` opens, as picked in the options.
pub fn default() -> &'static str {
    if Options::load().start == GUI {
        GUI
    } else {
        TUI
    }
}

/// The window's program, installed next to brumm.
pub const GUI_NAME: &str = "brumm-gui";

/// The running program; the installed path stays usable after Linux marks
/// the running executable as deleted following an atomic update.
pub fn executable() -> std::io::Result<PathBuf> {
    let p = std::env::current_exe()?;
    let s = p.to_string_lossy();
    Ok(match s.strip_suffix(" (deleted)") {
        Some(t) => PathBuf::from(t),
        None => p,
    })
}

/// Whether the installed program is no longer the one running. Release
/// timestamps are build times, so comparing modification dates fails; the
/// inode tells.
pub fn binary_updated() -> bool {
    executable().is_ok_and(|p| replaced(Path::new("/proc/self/exe"), &p))
}

fn replaced(running: &Path, installed: &Path) -> bool {
    let Ok(a) = fs::metadata(running) else {
        return false;
    };
    fs::metadata(installed).is_ok_and(|b| (a.dev(), a.ino()) != (b.dev(), b.ino()))
}

/// Finds the window: next to this brumm, else on the path.
pub fn gui_path() -> Result<PathBuf> {
    if let Ok(me) = std::env::current_exe() {
        let p = me.with_file_name(GUI_NAME);
        if fs::metadata(&p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0) {
            return Ok(p);
        }
    }
    which(GUI_NAME).ok_or_else(|| {
        err("brumm's window (brumm-gui) is not installed: brumm update brings it; from source, dev/setup builds it (needs qt6-declarative)")
    })
}

/// Starts the player as ui in a process of its own, apart from this one:
/// the window, or the terminal UI in a new terminal.
pub fn open(ui: &str) -> Result<()> {
    let ui = if ui.is_empty() { default() } else { ui };
    let me = std::env::current_exe()?;
    if std::env::var_os("BRUMM_PROTOTYPE").is_some_and(|v| !v.is_empty())
        || std::env::var_os("BRUMM_PREVIEW").is_some_and(|v| !v.is_empty())
    {
        return open_prototype(ui, &me);
    }
    if ui == GUI {
        gui_path()?; // say so here, before this player goes
        let mut cmd = Command::new(&me);
        cmd.arg("--gui");
        return detach(cmd, "gui.log");
    }
    // Omarchy's launcher styles the terminal, names it as the desktop
    // entry does, and focuses one already open; elsewhere any terminal the
    // desktop knows.
    let terminal = std::env::var("TERMINAL").unwrap_or_default();
    let me_s = me.to_string_lossy().to_string();
    for c in [
        vec!["omarchy-launch-or-focus-tui", me_s.as_str(), "--tui"],
        vec!["xdg-terminal-exec", "-e", me_s.as_str(), "--tui"],
        vec![terminal.as_str(), "-e", me_s.as_str(), "--tui"],
    ] {
        if c[0].is_empty() || which(c[0]).is_none() {
            continue;
        }
        let mut cmd = Command::new(c[0]);
        cmd.args(&c[1..]);
        return detach(cmd, "tui.log");
    }
    Err(err("no terminal to open brumm in"))
}

/// Runs cmd in a session of its own, so it outlives this process, its
/// output in the cache's log.
fn detach(mut cmd: Command, log: &str) -> Result<()> {
    let dir = config::cache_dir();
    fs::create_dir_all(&dir)?;
    let f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(dir.join(log))?;
    cmd.stdin(std::process::Stdio::null())
        .stdout(f.try_clone()?)
        .stderr(f);
    proc::detach(cmd)?;
    Ok(())
}

/// Connects to the daemon, starting it if needed.
pub async fn connect() -> Result<(Client, tokio::sync::mpsc::Receiver<Message>)> {
    if let Ok(c) = Client::dial().await {
        return Ok(c);
    }
    start_daemon().await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if let Ok(c) = Client::dial().await {
            return Ok(c);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(err(
        "the brumm daemon did not start; see `journalctl --user -u brumm`",
    ))
}

/// Prefers the systemd user unit, so the daemon is supervised and logs to
/// the journal; without it, it starts a detached daemon.
async fn start_daemon() -> Result<()> {
    let unit = tokio::process::Command::new("systemctl")
        .args(["--user", "start", "brumm.service"])
        .status()
        .await;
    if unit.is_ok_and(|s| s.success()) {
        return Ok(());
    }
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("daemon");
    detach(cmd, "daemon.log")
}

// The prototype and live preview carry their own socket, account copy and
// caches in the environment. Launch directly so switching works on the current
// desktop, regardless of compositor, workspace, tiling or window grouping.
fn preview_command(ui: &str, me: &Path) -> Result<Command> {
    if ui == GUI {
        return Ok(Command::new(gui_path()?));
    }
    if let Some(foot) = which("foot") {
        let mut cmd = Command::new(foot);
        cmd.args(["--app-id=brumm-prototype", "--title=brumm preview · TUI"])
            .arg(me)
            .arg("--tui");
        return Ok(cmd);
    }
    let terminal = std::env::var("TERMINAL").unwrap_or_default();
    for name in ["xdg-terminal-exec", terminal.as_str()] {
        if !name.is_empty()
            && let Some(program) = which(name)
        {
            let mut cmd = Command::new(program);
            cmd.arg("-e").arg(me).arg("--tui");
            return Ok(cmd);
        }
    }
    Err(err("no terminal to open the preview in"))
}

fn open_prototype(ui: &str, me: &Path) -> Result<()> {
    // Command inherits every preview variable, including XDG_DATA_HOME and
    // audio settings. No shell or compositor process can drop that context.
    let mut command = preview_command(ui, me)?;
    // Development tools set NO_COLOR for logs; a visual preview needs the
    // terminal's colors, including the foreground/background pairs in covers.
    command.env_remove("NO_COLOR");
    detach(command, "preview.log")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::testenv;

    #[test]
    fn replacement_ignores_build_timestamps() {
        let dir = std::env::temp_dir().join(format!("brumm-exe-{}", crate::engine::random_hex(6)));
        fs::create_dir_all(&dir).unwrap();
        let (running, installed) = (dir.join("running"), dir.join("installed"));
        fs::write(&running, "old").unwrap();
        fs::hard_link(&running, &installed).unwrap();
        assert!(
            !replaced(&running, &installed),
            "same inode reported as replaced"
        );
        let fresh = dir.join("new");
        fs::write(&fresh, "new").unwrap();
        fs::rename(&fresh, &installed).unwrap();
        assert!(replaced(&running, &installed), "replacement missed");
        fs::remove_file(&installed).unwrap();
        assert!(
            !replaced(&running, &installed),
            "a missing executable should not trigger a restart"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The window is found on the path, and missing it is said before
    /// anything starts.
    #[test]
    fn gui_on_the_path() {
        let Some(env) = testenv::isolated() else {
            return;
        };
        if std::env::current_exe()
            .unwrap()
            .with_file_name(GUI_NAME)
            .exists()
        {
            return; // a window built next to the tests
        }
        assert!(gui_path().is_err(), "found a window that is not there");
        assert!(open(GUI).is_err(), "opened a window that is not there");
        let p = env.dir.join("bin").join(GUI_NAME);
        fs::write(&p, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(gui_path().unwrap(), p);
    }

    /// Plain brumm opens what the options say, the terminal by default.
    #[test]
    fn default_face() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        assert_eq!(default(), TUI);
        let o = Options {
            start: GUI.into(),
            ..Default::default()
        };
        o.save().unwrap();
        assert_eq!(default(), GUI);
    }

    #[test]
    fn preview_switch_keeps_the_private_runtime_without_a_compositor() {
        let Some(env) = testenv::isolated() else {
            return;
        };
        let foot = env.dir.join("bin/foot");
        fs::write(&foot, r##"#!/bin/sh
printf '%s\n' "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "${NO_COLOR-unset}" "$@" > "$XDG_RUNTIME_DIR/preview-args"
"##).unwrap();
        fs::set_permissions(&foot, fs::Permissions::from_mode(0o755)).unwrap();
        let me = Path::new("/tmp/preview build/brumm");
        open_prototype(TUI, me).unwrap();
        let recorded = env.dir.join("run/preview-args");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !recorded.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let text = fs::read_to_string(recorded).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], env.dir.join("run").to_str().unwrap());
        assert_eq!(lines[1], env.dir.join("config").to_str().unwrap());
        assert_eq!(lines[2], env.dir.join("cache").to_str().unwrap());
        assert_eq!(lines[3], env.dir.join("data").to_str().unwrap());
        assert_eq!(lines[4], "unset", "preview inherited color suppression");
        assert_eq!(
            &lines[5..],
            [
                "--app-id=brumm-prototype",
                "--title=brumm preview · TUI",
                "/tmp/preview build/brumm",
                "--tui"
            ]
        );
    }

    #[test]
    fn preview_gui_launches_the_available_window_directly() {
        let Some(env) = testenv::isolated() else {
            return;
        };
        let gui = env.dir.join("bin").join(GUI_NAME);
        fs::write(&gui, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&gui, fs::Permissions::from_mode(0o755)).unwrap();
        let command = preview_command(GUI, Path::new("/tmp/preview/brumm")).unwrap();
        assert_eq!(command.get_program(), gui.as_os_str());
        assert_eq!(command.get_args().count(), 0);
    }
}
