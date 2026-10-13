//! The Omarchy bar widget, and the other Omarchy programs brumm asks.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use super::PLUGIN_ID;
use crate::error::{Result, err};

/// Where name is found on PATH, as an executable file.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let p = PathBuf::from(name);
        return is_exec(&p).then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| is_exec(p))
}

fn is_exec(p: &std::path::Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Runs `omarchy args…` and returns what it printed, or the last line of
/// it as the error.
fn omarchy(args: &[&str]) -> std::result::Result<String, String> {
    let out = Command::new("omarchy")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(text)
    }
}

/// Whether the omarchy command is here to manage the bar.
pub fn has_omarchy() -> bool {
    which("omarchy").is_some()
}

/// Whether the bar widget's files are where Omarchy looks.
pub fn plugin_linked() -> bool {
    std::fs::symlink_metadata(super::plugin_link()).is_ok()
}

fn last_line(out: &str) -> String {
    out.trim().lines().last().unwrap_or("").trim().to_string()
}

/// Turns the bar widget on or off.
pub fn set_plugin(on: bool) -> Result<()> {
    let verb = if on { "enable" } else { "disable" };
    match omarchy(&["plugin", verb, PLUGIN_ID]) {
        Ok(_) => Ok(()),
        Err(out) => {
            let msg = last_line(&out);
            if msg.is_empty() {
                return Err(err(format!("omarchy plugin {verb} failed")));
            }
            Err(err(msg
                .trim_start_matches(&format!("omarchy-plugin-{verb}: "))
                .to_string()))
        }
    }
}

/// Asks the Omarchy shell whether the bar widget is on; None when it
/// cannot tell (the shell is not running, or does not know it yet).
pub fn plugin_enabled() -> Option<bool> {
    #[derive(Deserialize)]
    struct Plugin {
        #[serde(default)]
        id: String,
        #[serde(default)]
        enabled: bool,
    }
    let out = omarchy(&["plugin", "list", "--json"]).ok()?;
    let plugins: Vec<Plugin> = serde_json::from_str(&out).ok()?;
    plugins
        .into_iter()
        .find(|p| p.id == PLUGIN_ID)
        .map(|p| p.enabled)
}

/// Has the Omarchy shell load the widget's files again.
pub(super) fn rescan() {
    if which("omarchy-shell").is_some() {
        let _ = Command::new("omarchy-shell")
            .args(["shell", "rescanPlugins"])
            .output();
    }
}

/// Whether Omarchy has a default coding agent chosen: feedback can be
/// handed to it.
pub fn has_agent() -> bool {
    if which("omarchy-agent").is_none() {
        return false;
    }
    Command::new("omarchy-default-agent")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
}
