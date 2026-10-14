//! The songs the daemon recently handed to players. A playback page that
//! cannot look songs up in batches (Spotify's) learns from them what its
//! queue looks like.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use crate::ipc::Message;
use crate::music::Track;

/// Songs kept; the oldest go first.
const CAP: usize = 5000;
/// Songs described per play: the start of a long list is what shows.
pub(super) const DESCRIBE_MAX: usize = 1500;

#[derive(Default)]
pub(super) struct Known {
    inner: Mutex<(HashMap<String, Track>, VecDeque<String>)>,
}

impl Known {
    pub fn note<'a>(&self, tracks: impl IntoIterator<Item = &'a Track>) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (map, order) = &mut *g;
        for t in tracks {
            if t.id.is_empty() {
                continue;
            }
            if map.insert(t.id.clone(), t.clone()).is_none() {
                order.push_back(t.id.clone());
            }
        }
        while order.len() > CAP {
            if let Some(old) = order.pop_front() {
                map.remove(&old);
            }
        }
    }

    /// Every song a reply carries.
    pub fn note_reply(&self, m: &Message) {
        self.note(&m.tracks);
        for s in &m.shelves {
            self.note(&s.tracks);
        }
        if let Some(p) = &m.explorer {
            self.note(&p.tracks);
            for s in &p.shelves {
                self.note(&s.tracks);
            }
        }
    }

    /// The known ones of ids, in their order, from the one at start on.
    pub fn of(&self, ids: &[String], start: usize, more: &[Track]) -> Vec<Track> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let extra: HashMap<&str, &Track> = more.iter().map(|t| (t.id.as_str(), t)).collect();
        ids.iter()
            .skip(start.min(ids.len()))
            .chain(ids.iter().take(start.min(ids.len())))
            .filter_map(|id| g.0.get(id).or_else(|| extra.get(id.as_str()).copied()))
            .take(DESCRIBE_MAX)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str) -> Track {
        Track {
            id: id.into(),
            title: id.to_uppercase(),
            ..Default::default()
        }
    }

    #[test]
    fn remembers_recent_songs_in_queue_order() {
        let k = Known::default();
        k.note(&[t("a"), t("b")]);
        let ids: Vec<String> = ["b", "x", "a", "c"].iter().map(|s| s.to_string()).collect();
        let got: Vec<String> = k.of(&ids, 2, &[t("c")]).into_iter().map(|t| t.id).collect();
        assert_eq!(
            got,
            ["a", "c", "b"],
            "from the start, then the songs before it"
        );
        let many: Vec<Track> = (0..CAP + 10).map(|i| t(&format!("s{i}"))).collect();
        k.note(&many);
        assert!(
            k.of(&["a".into()], 0, &[]).is_empty(),
            "the oldest went first"
        );
    }
}
