//! The explorer's actions: forms that create a playlist or folder or look
//! something up, a row's details page, favorites, and music videos, which
//! open in Apple Music.

use super::model::{Key, Model, Msg, Row, View};
use super::text::*;
use super::view::{MARGIN, box_lines};
use crate::ipc::{self, Request};
use crate::music::{Item, KIND_SHELF};

/// A one-line form for an action row.
#[derive(Clone, Debug, Default)]
pub struct FeatureForm {
    pub item: Item,
    pub input: String,
    /// The folder a new playlist or folder goes in.
    pub parent: String,
}

impl Model {
    pub fn provider_key(&mut self, key: &str) {
        match key {
            "q" | "Q" | "ctrl+c" => self.cmds.push(super::model::Cmd::Quit),
            "up" | "down" | "tab" | "k" | "j" => {
                let n = self.provider_choices.len();
                for _ in 0..n {
                    self.provider_index = if matches!(key, "up" | "k") {
                        (self.provider_index + n - 1) % n
                    } else {
                        (self.provider_index + 1) % n
                    };
                    if self.provider_choices[self.provider_index].available {
                        break;
                    }
                }
            }
            "enter" => {
                if let Some(p) = self
                    .provider_choices
                    .get(self.provider_index)
                    .filter(|p| p.available)
                {
                    self.ask(
                        Request {
                            cmd: ipc::CMD_PROVIDER.into(),
                            query: p.id.clone(),
                            ..Default::default()
                        },
                        |reply| {
                            Some(Msg::ProviderChoices(reply.and_then(|m| {
                                m.provider
                                    .ok_or_else(|| "missing provider selection".into())
                            })))
                        },
                    );
                }
            }
            _ => {}
        }
    }
    pub fn provider_box(&self) -> Vec<L> {
        let wd = 54.min(self.width - 2 * MARGIN - 4).max(8);
        let mut lines = vec![one(" Choose your music service", PLAIN)];
        for (i, p) in self.provider_choices.iter().enumerate() {
            lines.push(one(
                format!(
                    " {} {}{}",
                    if i == self.provider_index { ">" } else { " " },
                    p.name,
                    if p.available { "" } else { " (coming soon)" }
                ),
                if p.available { HERE } else { DIM },
            ));
        }
        if self.provider_choices.is_empty() {
            lines.push(one(" Loading providers…", DIM));
        }
        lines.push(one(" enter continue · q close", DIM));
        let h = lines.len() as i64 + 2;
        box_lines("Music provider", false, L::default(), lines, wd, h)
    }

    pub fn feature_key(&mut self, msg: &Key) {
        let Some(f) = self.feature_form.as_mut() else {
            return;
        };
        match msg.name.as_str() {
            "esc" => self.feature_form = None,
            "backspace" => {
                f.input.pop();
            }
            "ctrl+u" => f.input.clear(),
            "enter" => {
                let text = f.input.trim().to_string();
                if text.is_empty() {
                    return;
                }
                let Some(f) = self.feature_form.take() else {
                    return;
                };
                if let Some(what) = f.item.route.strip_prefix("action:lookup:") {
                    let mut it = f.item.clone();
                    it.route = format!("app:lookup:{what}:{text}");
                    it.id.clone_from(&it.route);
                    it.name = format!("Lookup: {text}");
                    let v = self.view(&it.name.clone(), &it.key(), Some(it));
                    return self.push(v, "");
                }
                let kind = f
                    .item
                    .route
                    .trim_start_matches("action:create:")
                    .to_string();
                self.ask(
                    Request {
                        cmd: ipc::CMD_CREATE.into(),
                        list: kind,
                        query: text,
                        start: f.parent,
                        ..Default::default()
                    },
                    |r| match r {
                        Err(e) => Some(Msg::Err(e)),
                        Ok(_) => Some(Msg::FeatureSaved),
                    },
                );
            }
            _ => f.input.push_str(&msg.text),
        }
    }

    pub fn feature_box(&self) -> Vec<L> {
        let Some(f) = &self.feature_form else {
            return Vec::new();
        };
        let wd = 54.min(self.width - 2 * MARGIN - 4).max(8);
        let fw = (wd - 3) as usize;
        let lines = vec![
            fit(
                &line(vec![sp(format!(" {}", f.input), PLAIN), sp("▏", HERE)]),
                fw,
            ),
            fit(&one(" enter save · esc cancel", DIM), fw),
        ];
        box_lines(&f.item.name, false, L::default(), lines, wd, 4)
    }

    /// Opens a page of the selected row's facts, with a way to add it to
    /// the favorites.
    pub fn inspect_selected(&mut self) {
        let v = self.cur();
        let Some(r) = v.rows.get(v.sel).cloned() else {
            return;
        };
        let (mut details, mut title, mut id) = (Vec::new(), String::new(), String::new());
        if let Some(t) = &r.track {
            details.clone_from(&t.details);
            title.clone_from(&t.title);
            id.clone_from(&t.id);
        }
        if let Some(it) = &r.item {
            details.clone_from(&it.details);
            title.clone_from(&it.name);
            id.clone_from(&it.id);
        }
        if title.is_empty() {
            return;
        }
        let mut page: View = self.view(
            &format!("{title} · details"),
            &format!("details:{id}"),
            None,
        );
        page.loaded = true;
        page.rows = details
            .iter()
            .map(|d| Row::note(format!("{}: {}", d.label, d.value)))
            .collect();
        if page.rows.is_empty() {
            page.rows
                .push(Row::note("No additional metadata available."));
        }
        let favorable = self.capabilities.favorites
            && (r.track.is_some()
                || r.item
                    .as_ref()
                    .is_some_and(|it| self.capabilities.favorite_kinds.contains(&it.kind)));
        if favorable {
            let kind = r
                .item
                .as_ref()
                .map_or("song".to_string(), |it| it.kind.clone());
            page.rows.push(Row::item(Item {
                kind: KIND_SHELF.into(),
                id,
                name: "Add favorite".into(),
                route: format!("action:favorite:{kind}"),
                ..Default::default()
            }));
        }
        self.push(page, "");
    }

    /// Music videos do not play here: they open in Apple Music, in the
    /// browser.
    pub fn open_music_video(&mut self, it: Item) {
        if it.url.is_empty() {
            let v = self.view(&it.name.clone(), &it.key(), Some(it));
            return self.push(v, "");
        }
        let ok =
            url::Url::parse(&it.url).is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some());
        if !ok {
            return self.set_flash("No video link available");
        }
        let link = it.url.clone();
        self.cmds
            .push(super::model::Cmd::Blocking(Box::new(move || {
                let r = std::process::Command::new("xdg-open")
                    .arg(&link)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                Some(match r {
                    Ok(s) if s.success() => Msg::Flash("Video opened in browser".into()),
                    Ok(s) => Msg::Err(format!("xdg-open: {s}")),
                    Err(e) => Msg::Err(format!("xdg-open: {e}")),
                })
            })));
    }
}
