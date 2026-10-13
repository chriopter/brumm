//! !: a box to write what is wrong. enter hands it to Omarchy's agent,
//! which looks into the logs and files the issue; without one, or with
//! tab, the browser opens on the issue filled in.

use super::model::{Key, Model, Msg};
use super::text::*;
use super::view::{MARGIN, box_lines};
use crate::ipc::{self, Request};

#[derive(Clone, Debug, Default)]
pub struct Feedback {
    pub input: String,
    pub sending: bool,
    /// Omarchy has an agent: asked once, when the box opens.
    pub agent: bool,
}

/// s broken into lines of at most width columns: at spaces where it can,
/// inside a word longer than a line where it must.
pub fn wrap_all(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let mut word = word.to_string();
            let sep = usize::from(!cur.is_empty());
            if sw(&cur) + sep + sw(&word) <= width {
                if sep == 1 {
                    cur.push(' ');
                }
                cur.push_str(&word);
                continue;
            }
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            while sw(&word) > width {
                let mut head = String::new();
                let mut rest = word.chars();
                for c in rest.by_ref() {
                    if sw(&head) + sw(c.encode_utf8(&mut [0; 4])) > width {
                        let mut tail = c.to_string();
                        tail.extend(rest);
                        out.push(head);
                        word = tail;
                        break;
                    }
                    head.push(c);
                }
            }
            cur = word;
        }
        out.push(cur);
    }
    out
}

impl Model {
    pub fn feedback_key(&mut self, msg: &Key) {
        let Some(f) = self.fb.as_mut() else { return };
        if f.sending {
            return;
        }
        let k = msg.name.as_str();
        match k {
            "esc" => self.fb = None,
            "enter" | "tab" => {
                let text = f.input.trim().to_string();
                if text.is_empty() {
                    return;
                }
                f.sending = true;
                // enter: the agent where there is one, else the browser; tab: the browser.
                let value = if k == "enter" && f.agent { 1.0 } else { 0.0 };
                let req = Request {
                    cmd: ipc::CMD_FEEDBACK.into(),
                    query: text,
                    source: "tui".into(),
                    value,
                    ..Default::default()
                };
                if self.client.is_none() {
                    return self.feedback_done(Err("not connected to the brumm daemon".into()));
                }
                self.ask(req, |r| Some(Msg::Feedback(r.map(|m| m.link))));
            }
            "backspace" => {
                f.input.pop();
            }
            "ctrl+u" => f.input.clear(),
            _ => f.input.push_str(&msg.text),
        }
    }

    pub fn feedback_done(&mut self, r: Result<String, String>) {
        self.fb = None;
        match r {
            Err(e) => self.set_flash(&e),
            Ok(how) if how == "agent" => self.set_flash("your agent is looking into it"),
            Ok(_) => self.set_flash("one click left: send it in your browser"),
        }
    }

    pub fn feedback_box(&self) -> Vec<L> {
        let Some(f) = &self.fb else { return Vec::new() };
        let wd = (self.width - 2 * MARGIN - 10).clamp(34, 64) as usize;
        // The words, wrapped to the box, the cursor after the last.
        let mut text = f.input.clone();
        if !f.sending {
            text.push('▏');
        }
        let mut lines: Vec<L> = wrap_all(&text, wd).into_iter().map(plain).collect();
        lines.push(L::default());
        if f.sending {
            lines.push(one("opening…", DIM));
        } else {
            let mut keys = line(vec![sp("enter", KEY), sp(" open on GitHub  ", DIM)]);
            if f.agent {
                keys = line(vec![
                    sp("enter", KEY),
                    sp(" send to my agent  ", DIM),
                    sp("tab", KEY),
                    sp(" open on GitHub  ", DIM),
                ]);
            }
            keys.spans.extend([sp("esc", KEY), sp(" cancel", DIM)]);
            lines.push(keys);
        }
        let wd = lines.iter().map(w).fold(wd, usize::max);
        let mut out = vec![L::default()];
        out.extend(lines.iter().map(|l| cat(plain(" "), fit(l, wd))));
        out.push(L::default());
        let n = out.len() as i64;
        box_lines(
            "send feedback or report an issue",
            false,
            L::default(),
            out,
            wd as i64 + 4,
            n + 2,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words_and_breaks_long_ones() {
        assert_eq!(
            wrap_all("the quick brown fox", 9),
            vec!["the quick", "brown fox"]
        );
        assert_eq!(wrap_all("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_all("", 4), vec![""]);
    }
}
