//! / in a list filters it where you are: the rows narrow as you type, over
//! titles, artists and albums, every word having to match. tab (or enter
//! with nothing left) takes the words to a search of all of Apple Music;
//! esc drops the filter. The Search tab and the queue keep / for search.

use std::sync::Arc;

use super::browse::nearest;
use super::model::{Key, Model, SEC_SEARCH, View};
use super::picker::drop_word;
use super::text::*;

/// Shows the rows of v that match its filter.
pub fn apply_filter(v: &mut View) {
    let Some(all) = &v.all else { return };
    let words: Vec<String> = v
        .filter
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if words.is_empty() {
        v.rows = all.clone();
    } else {
        v.rows = all
            .iter()
            .filter(|r| {
                let text = if let Some(t) = &r.track {
                    format!("{} {} {}", t.title, t.artist, t.album)
                } else if let Some(it) = &r.item {
                    format!("{} {}", it.name, it.artist)
                } else {
                    return false; // headings and notes do not survive a filter
                };
                let text = text.to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .cloned()
            .collect();
    }
    v.sel = nearest(&v.rows, 0, 1);
    v.off = 0;
}

/// Shows the whole list again, keeping the selected row.
pub fn clear_filter(v: &mut View) {
    let Some(all) = v.all.take() else { return };
    let keep = v.rows.get(v.sel).cloned();
    v.rows = all;
    v.filter.clear();
    fn same<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
        matches!((a, b), (Some(a), Some(b)) if Arc::ptr_eq(a, b))
    }
    if let Some(keep) = keep
        && let Some(i) = v
            .rows
            .iter()
            .position(|r| same(&r.track, &keep.track) || same(&r.item, &keep.item))
    {
        v.sel = i;
    }
}

impl Model {
    /// Whether / filters the list on screen.
    pub fn filterable(&self) -> bool {
        let v = self.cur();
        self.section != SEC_SEARCH
            && v.key != "queue:"
            && v.loaded
            && v.rows.len() + v.all.as_ref().map_or(0, Vec::len) > 0
    }

    /// Puts the cursor in the list's filter box.
    pub fn start_filter(&mut self) {
        let v = self.cur_mut();
        if v.all.is_none() {
            v.all = Some(v.rows.clone());
        }
        self.filtering = true;
        self.help = false;
    }

    /// Takes the filter's words to a search of Apple Music.
    fn search_everywhere(&mut self, q: String) {
        self.filtering = false;
        clear_filter(self.cur_mut());
        self.query = q;
        self.section = SEC_SEARCH;
        self.searching = false;
        self.run_search(true);
    }

    pub fn filter_key(&mut self, msg: &Key) {
        let k = msg.name.as_str();
        let v = self.cur_mut();
        match k {
            "esc" => {
                clear_filter(v);
                self.filtering = false;
                return;
            }
            "enter" => {
                if v.rows.is_empty() && !v.filter.trim().is_empty() {
                    let q = v.filter.clone();
                    return self.search_everywhere(q); // nothing here: look everywhere
                }
                self.filtering = false; // into the rows that are left
                return self.prefetch_cur();
            }
            "tab" => {
                if !v.filter.trim().is_empty() {
                    let q = v.filter.clone();
                    self.search_everywhere(q);
                }
                return;
            }
            "backspace" => {
                v.filter.pop();
            }
            "ctrl+w" => v.filter = drop_word(&v.filter),
            "ctrl+u" => v.filter.clear(),
            "ctrl+c" | "ctrl+q" => return self.quit(),
            "down" | "up" | "pgdown" | "pgup" => {
                self.filtering = false; // into the rows that are left
                return self.move_sel(k);
            }
            _ => {
                if msg.text.is_empty() {
                    return;
                }
                v.filter.push_str(&msg.text);
            }
        }
        apply_filter(self.cur_mut());
        self.prefetch_cur();
    }

    /// The line over a filtered list.
    pub fn filter_box(&self, wd: i64) -> L {
        let mut l = line(vec![
            sp("󰈲", HERE),
            sp(format!("  {}", self.cur().filter), PLAIN),
        ]);
        if self.filtering {
            l.spans.push(sp("▏", HERE));
            l.spans.extend([
                sp("  ", DIM),
                sp("tab", KEY),
                sp(format!(" all of {}  ", self.service()), DIM),
                sp("esc", KEY),
                sp(" clear", DIM),
            ]);
        } else {
            l.spans.extend([
                sp("  ", DIM),
                sp("/", KEY),
                sp(" edit  ", DIM),
                sp("esc", KEY),
                sp(" clear", DIM),
            ]);
        }
        fit(&l, wd.max(0) as usize)
    }
}
