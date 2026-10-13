//! The Omarchy bar widget: a switch in the options, and a question on the
//! first start when it is off. The installer turns it on for a fresh
//! install, so the question is for those who turned it off or never had it.

use serde_json::Map;

use super::model::{Cmd, Model, Msg};
use super::options::on_off;
use super::text::*;
use super::view::box_lines;
use crate::ipc::{self, Request};

impl Model {
    /// Asks the Omarchy shell whether the widget is on, off the loop.
    pub fn read_bar(&mut self) {
        if !self.has_omarchy {
            return;
        }
        self.cmds.push(Cmd::Blocking(Box::new(|| {
            Some(Msg::Bar(crate::update::plugin_enabled()))
        })));
    }

    /// Takes the shell's word (None: it cannot tell), and asks once when
    /// the widget is off.
    pub fn bar_read(&mut self, on: Option<bool>) {
        let Some(on) = on else { return };
        self.bar_on = on;
        if self.opts.o.bar != on_off(on) {
            self.opts.o.bar = on_off(on).into();
            self.save_options();
        }
        self.bar_ask = !on && !self.opts.o.bar_offered;
    }

    /// Switches the widget, showing the new state at once; bar_set takes
    /// it back if omarchy refuses.
    pub fn set_bar(&mut self, on: bool) {
        self.bar_on = on;
        self.opts.o.bar = on_off(on).into();
        match self.client.clone() {
            None => self.cmds.push(Cmd::Blocking(Box::new(move || {
                Some(Msg::BarSet {
                    on,
                    err: crate::update::set_plugin(on).err().map(|e| e.to_string()),
                })
            }))),
            Some(c) => self.task(async move {
                let mut set = Map::new();
                set.insert("bar".into(), on_off(on).into());
                let r = c
                    .request(Request {
                        cmd: ipc::CMD_OPTIONS.into(),
                        options: Some(set),
                        ..Default::default()
                    })
                    .await;
                Some(Msg::BarSet {
                    on,
                    err: r.err().map(|e| e.to_string()),
                })
            }),
        }
    }

    pub fn bar_set(&mut self, on: bool, err: Option<String>) {
        if let Some(e) = err {
            self.bar_on = !on;
            self.opts.o.bar = on_off(!on).into();
            self.save_options();
            self.set_flash(&format!("bar widget: {e}"));
            return;
        }
        self.set_flash(&format!("bar widget {}", on_off(on)));
    }

    /// The question shows once the player is past signing in, and never
    /// over another popup.
    pub fn bar_asking(&self) -> bool {
        self.bar_ask
            && self.state.status == ipc::STATUS_READY
            && self.upd.is_none()
            && self.pick.is_none()
            && !self.opt_open
            && !self.full
    }

    /// Whatever the answer, it is not asked again.
    fn bar_answer(&mut self, yes: bool) {
        self.bar_ask = false;
        self.opts.o.bar_offered = true;
        if yes {
            self.set_bar(true);
        }
        self.save_options();
    }

    /// enter or y says yes, esc or n no; other keys wait for an answer.
    pub fn bar_key(&mut self, k: &str) {
        match k {
            "enter" | "y" => self.bar_answer(true),
            "esc" | "n" | "q" => self.bar_answer(false),
            _ => {}
        }
    }

    /// A click on the question says yes, one outside no.
    pub fn bar_click(&mut self, x: i64, y: i64) {
        let yes = self.geo.options.has(x, y);
        self.bar_answer(yes);
    }

    pub fn bar_box(&self) -> Vec<L> {
        let lines = [
            one("Show brumm in the Omarchy bar?", BOLD),
            one("the options can change it later", DIM),
            L::default(),
            line(vec![
                sp("enter", KEY),
                sp(" yes  ", DIM),
                sp("esc", KEY),
                sp(" no", DIM),
            ]),
        ];
        framed("bar widget", &lines, 34)
    }
}

/// A popup box at least min_w wide around lines, a blank line above and
/// below them.
pub fn framed(title: &str, lines: &[L], min_w: usize) -> Vec<L> {
    let wd = lines.iter().map(|l| w(l) + 2).fold(min_w, usize::max);
    let mut out = vec![plain(" ".repeat(wd + 1))];
    out.extend(lines.iter().map(|l| cat(plain(" "), fit(l, wd))));
    out.push(plain(" ".repeat(wd + 1)));
    let n = out.len() as i64;
    box_lines(title, false, L::default(), out, wd as i64 + 4, n + 2)
}
