//! Omarchy opens its music apps with SUPER+SHIFT+M (Spotify) and
//! SUPER+SHIFT+ALT+M (a terminal player). brumm can take both over: its
//! window and its terminal player. It keeps the change to a file of its
//! own, loaded by one marked line in the user's bindings; switching it off
//! takes both away, and Omarchy's keys are back.

use std::fs;
use std::path::PathBuf;

use crate::config::home;
use crate::error::{Result, err};

fn hypr_dir() -> PathBuf {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d).join("hypr"),
        _ => home().join(".config/hypr"),
    }
}

fn keys_file() -> PathBuf {
    hypr_dir().join("brumm.lua")
}

fn bindings_file() -> PathBuf {
    hypr_dir().join("bindings.lua")
}

/// Loads brumm's keys from the user's bindings.
const KEYS_LINE: &str = r#"pcall(require, "hypr.brumm") -- brumm's music keys: brumm keys on|off"#;

const KEYS_LUA: &str = r#"-- Written by brumm (options → Music Keys, or brumm keys on|off); brumm
-- removes it again when switched off, and Omarchy's music keys are back.
hl.unbind("SUPER + SHIFT + M")
hl.unbind("SUPER + SHIFT + ALT + M")
o.bind("SUPER + SHIFT + M", "brumm", "brumm open gui")
o.bind("SUPER + SHIFT + ALT + M", "brumm in the terminal", "brumm open tui")
"#;

/// Whether this Hyprland is set up the Omarchy way, with the user's
/// bindings in bindings.lua.
pub fn can_music_keys() -> bool {
    bindings_file().exists()
}

/// Whether brumm has the music keys now.
#[cfg(test)]
fn music_keys_on() -> bool {
    fs::read_to_string(bindings_file()).is_ok_and(|b| b.contains(KEYS_LINE)) && keys_file().exists()
}

/// Gives the music keys to brumm, or back to Omarchy, and has Hyprland
/// read its config again.
pub fn set_music_keys(on: bool) -> Result<()> {
    if !can_music_keys() {
        return Err(err(
            "music keys: no ~/.config/hypr/bindings.lua (Omarchy's Hyprland config)",
        ));
    }
    let b = fs::read_to_string(bindings_file())?;
    let mut text = b.clone();
    let has = text.contains(KEYS_LINE);
    if on {
        fs::write(keys_file(), KEYS_LUA)?;
        if !has {
            if !text.ends_with('\n') && !text.is_empty() {
                text.push('\n');
            }
            text.push('\n');
            text.push_str(KEYS_LINE);
            text.push('\n');
        }
    } else {
        let _ = fs::remove_file(keys_file());
        if has {
            let added = format!("\n{KEYS_LINE}\n");
            if text.contains(&added) {
                text = text.replacen(&added, "", 1); // as it was added
            } else {
                text = text.replacen(&format!("{KEYS_LINE}\n"), "", 1);
            }
        }
    }
    if text != b {
        fs::write(bindings_file(), text)?;
    }
    if super::plugin::which("hyprctl").is_some() {
        let _ = std::process::Command::new("hyprctl").arg("reload").output();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::testenv;

    /// The keys go to brumm with a file and one line, and back with neither.
    #[test]
    fn music_keys_round_trip() {
        let Some(env) = testenv::isolated() else {
            return;
        };
        assert!(
            set_music_keys(true).is_err() && !can_music_keys(),
            "set keys without Omarchy's bindings"
        );
        let mine = "-- my keys\no.bind(\"SUPER + X\", nil, \"x\")\n";
        fs::create_dir_all(env.dir.join("config/hypr")).unwrap();
        fs::write(bindings_file(), mine).unwrap();
        for _ in 0..2 {
            set_music_keys(true).unwrap(); // twice: no second line
        }
        let b = fs::read_to_string(bindings_file()).unwrap();
        assert!(
            music_keys_on() && b.matches(KEYS_LINE).count() == 1 && b.starts_with(mine),
            "on: {b:?}"
        );
        set_music_keys(false).unwrap();
        let b = fs::read_to_string(bindings_file()).unwrap();
        assert!(!music_keys_on() && b == mine, "off: {b:?}");
        assert!(!keys_file().exists(), "brumm.lua left behind");
    }
}
