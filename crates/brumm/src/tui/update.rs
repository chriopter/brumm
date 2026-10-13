//! U: checks for a release right then and says what it found — the newest
//! already, or a newer one to install with enter, its notes listed.

use super::bar::framed;
use super::model::{Model, Msg};
use super::text::*;
use super::view::SPINNER;
use crate::ipc::{self, Message, Request};

#[derive(Clone, Debug, Default)]
pub struct UpdatePopup {
    pub checking: bool,
    pub installing: bool,
    /// Done: offer to restart into it now.
    pub installed: bool,
    pub current: String,
    pub latest: String,
    pub newer: bool,
    /// What the newer release says is new.
    pub notes: Vec<String>,
    pub err: String,
}

/// The box shows this much of the release's notes.
const MAX_NOTES: usize = 12;
const NOTE_WIDTH: i64 = 64;

impl Model {
    pub fn check_update(&mut self) {
        if self.upd.as_ref().is_some_and(|u| u.newer && !u.installing) {
            return self.install_update();
        }
        self.upd = Some(UpdatePopup {
            checking: true,
            ..Default::default()
        });
        self.opt_open = false;
        self.pick = None;
        self.help = false;
        if self.client.is_none() {
            return self.update_checked(Err("not connected to the brumm daemon".into()));
        }
        self.ask(
            Request {
                cmd: ipc::CMD_UPDATE.into(),
                value: 2.0,
                ..Default::default()
            },
            |r| Some(Msg::UpdateChecked(r)),
        );
    }

    pub fn update_checked(&mut self, r: Result<Message, String>) {
        let Some(u) = self.upd.as_mut() else { return };
        u.checking = false;
        match r {
            Ok(m) => {
                u.current = m.version;
                u.latest = m.link;
                u.newer = m.pos == 1;
                u.notes = m.notes;
            }
            Err(e) => u.err = e,
        }
    }

    fn install_update(&mut self) {
        if let Some(u) = self.upd.as_mut() {
            u.installing = true;
        }
        self.ask(
            Request {
                cmd: ipc::CMD_UPDATE.into(),
                value: 1.0,
                ..Default::default()
            },
            |r| Some(Msg::Installed(r.map(|_| ()))),
        );
    }

    pub fn update_installed(&mut self, r: Result<(), String>) {
        if let Err(e) = r {
            self.upd = None;
            return self.set_flash(&format!("update: {e}"));
        }
        let u = self.upd.get_or_insert_with(UpdatePopup::default);
        u.installing = false;
        u.installed = true;
    }

    /// Has the player restart into the new release at once; it picks up
    /// where it was, and this player follows (Msg::Closed).
    fn restart_now(&mut self) {
        self.upd = None;
        self.set_flash("restarting…");
        self.ask(
            Request {
                cmd: ipc::CMD_UPDATE.into(),
                value: 3.0,
                ..Default::default()
            },
            |r| r.err().map(Msg::Err),
        );
    }

    /// enter installs a newer release, anything else closes.
    pub fn update_key(&mut self, k: &str) {
        let Some(u) = &self.upd else { return };
        let close = matches!(k, "esc" | "q" | "space" | " ");
        if u.installing {
        } else if u.installed && (k == "enter" || k == "U") {
            self.restart_now();
        } else if u.installed {
            if close {
                self.upd = None;
                self.set_flash("brumm restarts into it at the next pause or song change");
            }
        } else if (k == "enter" || k == "U") && u.newer {
            self.install_update();
        } else if close || k == "enter" || k == "U" {
            self.upd = None;
        }
    }

    /// A click outside closes, on the popup installs if it can.
    pub fn update_click(&mut self, x: i64, y: i64) {
        if !self.geo.options.has(x, y) {
            if self.upd.as_ref().is_some_and(|u| !u.installing) {
                self.upd = None;
            }
            return;
        }
        self.update_key("enter");
    }

    pub fn update_box(&self) -> Vec<L> {
        let Some(u) = &self.upd else {
            return Vec::new();
        };
        let spin = SPINNER[self.frame.rem_euclid(SPINNER.len() as i64) as usize];
        let mut lines: Vec<L> = Vec::new();
        if u.checking {
            lines.push(one(format!("{spin}  looking for a new release…"), DIM));
        } else if u.installing {
            lines.push(one(format!("{spin}  installing {}…", u.latest), DIM));
        } else if u.installed {
            let name = if u.latest.is_empty() {
                "the update".to_string()
            } else {
                u.latest.clone()
            };
            lines.push(line(vec![
                sp("✓", PLAYS),
                sp(" ", PLAIN),
                sp(name, BOLD),
                sp(" is installed", PLAIN),
            ]));
            lines.push(one("the music goes on where it is", DIM));
            lines.push(L::default());
            lines.push(line(vec![
                sp("enter", KEY),
                sp(" restart now  ", DIM),
                sp("esc", KEY),
                sp(" at the next pause", DIM),
            ]));
        } else if !u.err.is_empty() {
            lines.push(one(u.err.clone(), ERR));
            lines.push(L::default());
            lines.push(line(vec![sp("esc", KEY), sp(" close", DIM)]));
        } else if u.newer {
            lines.push(line(vec![
                sp(u.latest.clone(), BOLD),
                sp(" is available", PLAIN),
            ]));
            lines.push(one(format!("you have {}", u.current), DIM));
            lines.push(L::default());
            let width = NOTE_WIDTH.min((self.width - 12).max(34)) as usize;
            for n in u.notes.iter().take(MAX_NOTES) {
                if let Some(h) = n.strip_prefix("# ") {
                    lines.push(one(h, BOLD));
                    continue;
                }
                for (i, l) in wrap(n, width, 3).into_iter().enumerate() {
                    lines.push(plain(if i > 0 { format!("   {l}") } else { l })); // under the words, past the emoji
                }
            }
            if u.notes.len() > MAX_NOTES {
                lines.push(one("… and more", DIM));
            }
            if !u.notes.is_empty() {
                lines.push(L::default());
            }
            lines.push(line(vec![
                sp("enter", KEY),
                sp(" install  ", DIM),
                sp("esc", KEY),
                sp(" later", DIM),
            ]));
        } else {
            lines.push(line(vec![
                sp("✓", PLAYS),
                sp(" ", PLAIN),
                sp(u.current.clone(), BOLD),
                sp(" is the newest version", PLAIN),
            ]));
            lines.push(L::default());
            lines.push(line(vec![sp("esc", KEY), sp(" close", DIM)]));
        }
        framed("update", &lines, 34)
    }
}
