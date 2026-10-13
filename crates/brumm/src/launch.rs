//! Starts brumm's pieces: the daemon behind the player, and the player
//! itself as the terminal UI or the window (GUI).

use std::fs;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

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
        err("brumm's window (brumm-gui) is not installed: brumm update brings it; from source, bin/setup builds it (needs qt6-declarative)")
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

// ── the prototype and the live preview (bin/prototype, bin/preview) ──────
// They share their socket across both faces and stay on the testing
// workspace, including switches with g.

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct PreviewWindow {
    address: String,
    class: String,
    title: String,
    mapped: bool,
    floating: bool,
    grouped: Vec<String>,
    workspace: Workspace,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct Workspace {
    id: i64,
}

fn preview_hypr(args: &[&str]) -> Result<Vec<u8>> {
    let runtime = std::env::var("BRUMM_DESKTOP_RUNTIME").unwrap_or_default();
    let out = Command::new("hyprctl")
        .args(args)
        .env("XDG_RUNTIME_DIR", runtime)
        .output()?;
    if !out.status.success() {
        return Err(err(format!(
            "hyprctl: {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        )));
    }
    Ok(out.stdout)
}

fn preview_windows() -> Result<Vec<PreviewWindow>> {
    Ok(serde_json::from_slice(&preview_hypr(&["clients", "-j"])?)?)
}

fn is_preview_face(w: &PreviewWindow, ui: &str) -> bool {
    if ui == GUI {
        return w.class == "brumm"
            && (w.title == "brumm · API prototype · sample data"
                || w.title == "brumm · API preview · live account");
    }
    w.class == "brumm-prototype"
}

fn preview_source(ws: &[PreviewWindow], ui: &str) -> Option<PreviewWindow> {
    ws.iter()
        .find(|w| {
            w.mapped
                && w.workspace.id == 4
                && !is_preview_face(w, ui)
                && (is_preview_face(w, GUI) || is_preview_face(w, TUI))
                && !w.floating
                && w.grouped.is_empty()
        })
        .cloned()
}

fn await_preview(ws: &[PreviewWindow], ui: &str) -> Result<PreviewWindow> {
    let seen: std::collections::HashSet<&str> = ws.iter().map(|w| w.address.as_str()).collect();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Some(w) = preview_windows()?
            .into_iter()
            .find(|w| w.mapped && !seen.contains(w.address.as_str()) && is_preview_face(w, ui))
        {
            return Ok(w);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(err(
        "the prototype window did not appear; keeping the current player",
    ))
}

/// Stages the new window off-screen, then inserts it at the old focused
/// leaf and removes the old window in the same compositor batch.
fn handoff_preview(old: &PreviewWindow, next: &PreviewWindow) -> Result<()> {
    let active: PreviewWindow = preview_hypr(&["activewindow", "-j"])
        .ok()
        .and_then(|o| serde_json::from_slice(&o).ok())
        .unwrap_or_default();
    let sel = |w: &PreviewWindow| {
        serde_json::to_string(&format!("address:{}", w.address)).unwrap_or_default()
    };
    let focus_after = if !active.address.is_empty() && active.address != old.address {
        &active
    } else {
        next
    };
    let batch = [
        format!("dispatch hl.dsp.focus({{ window = {} }})", sel(old)),
        format!(
            "dispatch hl.dsp.window.move({{ window = {}, workspace = \"4\", follow = false }})",
            sel(next)
        ),
        format!(
            "dispatch hl.dsp.window.move({{ window = {}, workspace = \"special:brumm-handoff\", follow = false }})",
            sel(old)
        ),
        format!("dispatch hl.dsp.focus({{ window = {} }})", sel(focus_after)),
    ];
    preview_hypr(&["--batch", &batch.join("; ")])?;
    Ok(())
}

fn open_prototype(ui: &str, me: &Path) -> Result<()> {
    let mut args: Vec<String> = vec!["env".into()];
    for key in [
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "WAYLAND_DISPLAY",
        "BRUMM_DESKTOP_RUNTIME",
        "BRUMM_PROTOTYPE",
        "BRUMM_PREVIEW",
        "PULSE_SERVER",
        "PIPEWIRE_RUNTIME_DIR",
    ] {
        args.push(format!("{key}={}", std::env::var(key).unwrap_or_default()));
    }
    if ui == GUI {
        args.push(gui_path()?.to_string_lossy().into_owned());
    } else {
        let foot = which("foot").ok_or_else(|| err("prototype terminal: foot not found"))?;
        args.extend([
            foot.to_string_lossy().into_owned(),
            "--app-id=brumm-prototype".into(),
            "--title=brumm prototype · TUI".into(),
            me.to_string_lossy().into_owned(),
            "--tui".into(),
        ]);
    }
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none_or(|v| v.is_empty()) {
        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);
        return detach(cmd, "prototype.log");
    }
    let quoted: Vec<String> = args
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', r"'\''")))
        .collect();
    let windows = preview_windows()?;
    let source = preview_source(&windows, ui);
    let workspace = if source.is_some() {
        "special:brumm-handoff silent"
    } else {
        "4 silent"
    };
    let quote = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let expr = format!(
        "hl.dsp.exec_cmd({}, {{ workspace = {}, no_initial_focus = true }})",
        quote(&format!("exec {}", quoted.join(" "))),
        quote(workspace)
    );
    preview_hypr(&["dispatch", &expr])?;
    let next = await_preview(&windows, ui)?;
    if let Some(src) = source {
        handoff_preview(&src, &next)?;
    }
    Ok(())
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
    fn preview_source_only_selects_an_eligible_prototype() {
        let normal = PreviewWindow {
            address: "old".into(),
            class: "brumm".into(),
            title: "brumm · API prototype · sample data".into(),
            mapped: true,
            workspace: Workspace { id: 4 },
            ..Default::default()
        };
        let mut unrelated = normal.clone();
        unrelated.title = "Real music · brumm".into();
        let mut floating = normal.clone();
        floating.floating = true;
        let mut grouped = normal.clone();
        grouped.grouped = vec!["other".into()];
        let mut hidden = normal.clone();
        hidden.workspace.id = -99;
        for w in [&unrelated, &floating, &grouped, &hidden] {
            assert!(
                preview_source(std::slice::from_ref(w), TUI).is_none(),
                "ineligible window chosen: {w:?}"
            );
        }
        assert_eq!(
            preview_source(&[unrelated, normal.clone()], TUI)
                .unwrap()
                .address,
            "old"
        );
        assert!(
            preview_source(&[normal], GUI).is_none(),
            "selected the same face as the replacement"
        );
    }
}
