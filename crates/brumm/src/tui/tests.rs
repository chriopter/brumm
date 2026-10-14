//! The player's behavior, driven through its messages without a terminal.
//! Every test that builds a model runs isolated: options, the split and
//! covers live in a home of its own, and no desktop command is found.

use std::sync::Arc;
use std::time::{Duration, Instant};

use image::RgbaImage;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect as Area;

use super::browse::fill;
use super::model::*;
use super::options::{
    COVER_ORIGINAL, COVER_PIXEL, COVER_SMOOTH, OPT_COLORS, OPT_COVER, OPT_KEYS, OPT_MOTION,
};
use super::text::*;
use super::update::UpdatePopup;
use super::view::BODY_TOP;
use crate::art::Size;
use crate::config::testenv;
use crate::ipc::{self, Message, State};
use crate::music::{Item, KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_STATION, Shelf, Track};

fn ready() -> State {
    State {
        status: ipc::STATUS_READY.into(),
        ..Default::default()
    }
}

/// A w×h screen, nothing playing.
fn model(w: i64, h: i64) -> Model {
    let mut m = Model::new(None, ready());
    use crate::provider::session::Definition;
    m.configure_navigation(crate::provider::session::navigation(
        crate::provider::apple::session::Apple.navigation(),
    ));
    m.capabilities = crate::provider::apple::session::Apple.capabilities();
    m.section = 0;
    m.provider_name = "Apple Music".into();
    m.width = w;
    m.height = h;
    m.split = 0.5;
    m
}

/// A w×h screen playing a song with a two-color cover.
fn playing(w: i64, h: i64) -> Model {
    let mut m = model(w, h);
    let p = &mut m.state.player;
    (p.title, p.artist, p.album, p.id, p.dur) = (
        "Midnight City".into(),
        "M83".into(),
        "Hurry Up".into(),
        "1".into(),
        240.0,
    );
    let img = RgbaImage::from_fn(60, 60, |x, y| {
        image::Rgba([(200 * x / 60) as u8, 40, (200 * y / 60) as u8, 255])
    });
    m.cover_url = "now.jpg".into();
    m.cover = Some(Arc::new(img));
    m
}

fn track(id: &str, title: &str) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        ..Default::default()
    }
}

fn tracks(ts: Vec<Track>) -> Message {
    Message {
        tracks: ts,
        ..Default::default()
    }
}

fn press(m: &mut Model, name: &str) {
    m.update(Msg::Key(Key::named(name)));
}

fn type_in(m: &mut Model, s: &str) {
    for c in s.chars() {
        press(m, &c.to_string());
    }
}

fn click(m: &mut Model, x: i64, y: i64) {
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Down,
        button: Button::Left,
        x,
        y,
    }));
}

/// The screen as drawn, a line of text per row.
fn screen(m: &mut Model) -> Vec<String> {
    let mut buf = Buffer::empty(Area::new(0, 0, m.width as u16, m.height as u16));
    m.draw(&mut buf);
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn shows(m: &mut Model, s: &str) -> bool {
    screen(m).iter().any(|l| l.contains(s))
}

/// The lines render made, failing on one wider than the screen.
fn widths(m: &mut Model) -> Vec<usize> {
    let lines = m.render();
    assert!(
        lines.len() as i64 <= m.height,
        "{} lines for a {} high screen",
        lines.len(),
        m.height
    );
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            assert!(
                w(l) as i64 <= m.width,
                "line {i} is {} wide on a {} wide screen",
                w(l),
                m.width
            );
            w(l)
        })
        .collect()
}

/// The playing song and n more of its album, at queue position 4.
fn album_queue(m: &Model, n: usize) -> Vec<Track> {
    let mut q = vec![Track {
        id: m.state.player.id.clone(),
        title: m.state.player.title.clone(),
        album: "Hurry Up".into(),
        artist: "M83".into(),
        ..Default::default()
    }];
    for i in 1..=n {
        q.push(Track {
            id: format!("a{i}"),
            title: format!("Song {i}"),
            album: "Hurry Up".into(),
            artist: "M83".into(),
            number: i as i64 + 1,
            duration: 200.0,
            ..Default::default()
        });
    }
    q
}

// ── no model: pure pieces ──────────────────────────────────────────────

/// Section numbers are exchanged in CMD_PLACE when g hands off a view;
/// both players must mean the same by them.
#[test]
fn section_order_matches_the_gui() {
    let qml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../gui/qml/Store.qml"
    ))
    .expect("gui/qml/Store.qml");
    use crate::provider::session::Definition;
    let entries =
        crate::provider::session::navigation(crate::provider::apple::session::Apple.navigation());
    assert_eq!(
        entries.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(),
        SECTION_NAMES
    );
    assert!(qml.contains("sectionNames: navigation.map(e => e.label)"));
    assert!(qml.contains("sectionId: navigation[section].id"));
}

#[test]
fn provider_navigation_can_reorder_rename_and_omit_sections() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 40);
    let mut entries = m.navigation[..7].to_vec();
    entries.retain(|entry| entry.id != "radio");
    entries.swap(0, SEC_ARTISTS);
    entries[0].label = "My Artists".into();
    m.section = SEC_ALBUMS;
    m.configure_navigation(crate::provider::session::navigation(entries));
    assert_eq!(m.navigation[0].label, "My Artists");
    assert_eq!(m.navigation[m.section].id, "albums");
    assert_eq!(m.sec("artists"), 0);
    assert_eq!(m.navigation.len(), SEC_QUEUE);
    assert!(!m.navigation.iter().any(|e| e.id == "radio"));
    let place = m.here();
    assert_eq!(place.section_id, "albums");
    m.switch_to(m.sec("queue"));
    m.go_place(place);
    assert_eq!(m.navigation[m.section].id, "albums");
}

#[test]
fn provider_capabilities_gate_actions_without_service_checks() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 40);
    m.capabilities = Default::default();
    m.rate(1);
    m.play_station();
    m.toggle_preview();
    assert!(m.rating.is_empty());
    assert!(!m.previewing);
}

/// Terminal keys get the names the handlers match.
#[test]
fn key_names() {
    let k = |code, mods, kind| {
        let Some(Msg::Key(k)) = super::to_msg(Event::Key(KeyEvent {
            code,
            modifiers: mods,
            kind,
            state: KeyEventState::NONE,
        })) else {
            panic!("no key")
        };
        k
    };
    let none = KeyModifiers::NONE;
    let press = KeyEventKind::Press;
    assert_eq!(k(KeyCode::Char('a'), none, press).name, "a");
    let a = k(KeyCode::Char('a'), KeyModifiers::SHIFT, press);
    assert_eq!(
        (a.name.as_str(), a.text.as_str()),
        ("A", "A"),
        "kitty reports shift+a as a with shift"
    );
    assert_eq!(
        k(KeyCode::Char('c'), KeyModifiers::CONTROL, press).name,
        "ctrl+c"
    );
    let space = k(KeyCode::Char(' '), none, KeyEventKind::Release);
    assert!(space.name == "space" && space.text == " " && space.release);
    assert!(k(KeyCode::Char(' '), none, KeyEventKind::Repeat).repeat);
    assert_eq!(
        k(KeyCode::BackTab, KeyModifiers::SHIFT, press).name,
        "shift+tab"
    );
    assert_eq!(
        k(KeyCode::Left, KeyModifiers::SHIFT, press).name,
        "shift+left"
    );
    assert_eq!(k(KeyCode::PageDown, none, press).name, "pgdown");
    assert_eq!(k(KeyCode::Char('*'), KeyModifiers::SHIFT, press).name, "*");
    assert_eq!(k(KeyCode::Esc, none, press).name, "esc");
    let ms = |kind| {
        super::to_msg(Event::Mouse(MouseEvent {
            kind,
            column: 3,
            row: 4,
            modifiers: KeyModifiers::NONE,
        }))
    };
    assert!(matches!(
        ms(MouseEventKind::Moved),
        Some(Msg::Mouse(Mouse {
            kind: MouseKind::Moved,
            button: Button::None,
            x: 3,
            y: 4
        }))
    ));
    assert!(matches!(
        ms(MouseEventKind::Drag(MouseButton::Left)),
        Some(Msg::Mouse(Mouse {
            kind: MouseKind::Moved,
            button: Button::Left,
            ..
        }))
    ));
    assert!(matches!(
        ms(MouseEventKind::ScrollDown),
        Some(Msg::Mouse(Mouse {
            kind: MouseKind::WheelDown,
            ..
        }))
    ));
}

/// The startup probe reads the background and the keyboard protocol, and
/// knows the device attributes end it.
#[test]
fn probe_answers() {
    let (bg, kitty, done) =
        super::parse_probe(b"\x1b]11;rgb:1a1a/2b2b/ffff\x1b\\\x1b[?1u\x1b[?62;22c");
    assert_eq!(bg, Some([0x1a, 0x2b, 0xff]));
    assert!(kitty && done);
    let (bg, kitty, done) = super::parse_probe(b"\x1b]11;rgb:ff/00/80\x07\x1b[?64;1;9");
    assert_eq!(bg, Some([255, 0, 128]));
    assert!(!kitty && !done, "the attributes are not complete yet");
    assert_eq!(super::parse_probe(b"\x1b[?6c"), (None, false, true));
}

/// Kitty placeholder cells measure one cell each, cut or whole.
#[test]
fn kitty_placeholder_width() {
    let size = Size {
        width: 7,
        height: 3,
    };
    for row in crate::art::kitty_lines(size) {
        let l = one(row.concat(), PLAIN);
        assert_eq!(w(&l), 7);
        assert_eq!(w(&cut(&l, 2, 5)), 3);
    }
}

// ── the model ──────────────────────────────────────────────────────────

/// Headings and notes are read, never selected: the cursor steps over them.
#[test]
fn shelves_skip_headings() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 40);
    assert_eq!(m.cur().key, "home:", "brumm opens on Home");
    let album = Item {
        kind: KIND_ALBUM.into(),
        id: "l.1".into(),
        name: "Blue".into(),
        ..Default::default()
    };
    let station = Item {
        kind: KIND_STATION.into(),
        id: "ra.1".into(),
        name: "Station".into(),
        ..Default::default()
    };
    fill(
        m.cur_mut(),
        Message {
            shelves: vec![
                Shelf {
                    title: "Recently played".into(),
                    items: vec![album],
                    ..Default::default()
                },
                Shelf {
                    title: "Radio".into(),
                    items: vec![station],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    );
    let name = |m: &Model| {
        m.cur().rows[m.cur().sel]
            .item
            .as_ref()
            .map(|i| i.name.clone())
            .unwrap_or_default()
    };
    assert_eq!(name(&m), "Blue");
    m.move_sel("down");
    assert_eq!(name(&m), "Station");
    m.move_sel("up");
    assert_eq!(name(&m), "Blue");
    m.move_sel("home");
    assert!(m.cur().rows[m.cur().sel].selectable());
    assert!(
        shows(&mut m, "Recently played") && shows(&mut m, "Radio"),
        "shelf headings are drawn"
    );
}

/// The tabs fit even the narrowest list, and every screen size keeps its
/// lines inside the window.
#[test]
fn layout_fits() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    for (wd, h) in [
        (60, 30),
        (90, 30),
        (140, 30),
        (40, 14),
        (200, 55),
        (300, 80),
    ] {
        let mut m = model(wd, h);
        widths(&mut m);
        let g = m.geo.tabs[SEC_SEARCH];
        assert!(
            g.x1 > 0 && g.x1 <= wd,
            "width {wd}: the search tab is at {g:?}"
        );
        for split in [0.1, 0.3, 0.5, 0.8, 0.95] {
            let mut m = playing(wd, h);
            m.split = split;
            let ws = widths(&mut m);
            // The stage ends at the right margin: no line stops short of it
            // (narrower, the mini player stands under the list instead).
            let body = &ws[1..ws.len() - 2];
            assert!(
                wd < 90 || body.iter().all(|&x| x as i64 == wd - super::view::MARGIN),
                "{wd}x{h} at {split}: body lines end at the margin: {ws:?}"
            );
        }
    }
    let mut m = model(38, 10);
    assert!(shows(&mut m, "needs a bigger window"));
}

/// The divider follows the mouse and sizes the cover; it is remembered.
#[test]
fn divider_drags() {
    let Some(env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(160, 45);
    m.render();
    let d = m.geo.divider;
    assert!(d.x1 > d.x0, "a divider is drawn");
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Down,
        button: Button::Left,
        x: d.x0 + 1,
        y: d.y0 + 2,
    }));
    assert!(m.dragging);
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Moved,
        button: Button::Left,
        x: d.x0 + 21,
        y: d.y0 + 2,
    }));
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Up,
        button: Button::Left,
        x: d.x0 + 21,
        y: d.y0 + 2,
    }));
    assert!(!m.dragging);
    let split = m.split;
    assert!(split > 0.5, "dragging right widens the list: {split}");
    let saved =
        std::fs::read_to_string(env.dir.join("cache/brumm/split")).expect("the split is saved");
    assert_eq!(saved, format!("{split:.3}"));
    assert_eq!(load_split(), (split * 1000.0).round() / 1000.0);
}

/// ? opens and closes the key list; in the search box it is a letter.
#[test]
fn help_key() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    press(&mut m, "?");
    assert!(m.help && !m.show_tips());
    assert!(shows(&mut m, "fullscreen visualizer"));
    press(&mut m, "?");
    assert!(!m.help);
    m.opts.o.always_tips = true;
    assert!(m.show_tips(), "the option puts keys on the buttons");
    m.searching = true;
    press(&mut m, "?");
    assert_eq!(m.query, "?");
}

/// Every key the help names does something.
#[test]
fn help_keys_work() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 40);
    fill(
        m.cur_mut(),
        tracks(vec![track("1", "Song"), track("1", "Song")]),
    );
    let p = &mut m.state.player;
    (p.id, p.title, p.dur) = ("1".into(), "Song".into(), 100.0);
    for k in [
        "o", "O", "i", "*", "d", "R", "P", "y", "z", "Z", "a", "A", "s", "r", "n", "p", "+", "-",
        "m", "f", "q", "Q",
    ] {
        m.opt_open = false;
        m.pick = None;
        m.full = false;
        m.cmds.clear();
        let (flash, state) = (m.flash.clone(), m.state.clone());
        m.key(k);
        let changed = m.opt_open
            || m.pick.is_some()
            || m.full
            || m.flash != flash
            || m.state != state
            || m.previewing;
        assert!(!m.cmds.is_empty() || changed, "key {k:?} does nothing");
    }
}

/// ← and → leave the search box for the next section; a click on a tab
/// leaves it too, and the next key is a key again.
#[test]
fn leaving_search() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.switch_to(SEC_SEARCH);
    assert!(!m.searching, "arriving at search does not focus its box");
    m.key("/");
    press(&mut m, "right");
    assert_eq!(m.section, SEC_HOME);
    m.switch_to(SEC_SEARCH);
    m.key("/");
    press(&mut m, "left");
    assert_eq!(m.section, SEC_RADIO);

    m.switch_to(SEC_SEARCH);
    m.key("/");
    assert!(
        shows(&mut m, "esc done"),
        "the focused box says esc leaves it"
    );
    let r = m.geo.tabs[SEC_HOME];
    click(&mut m, r.x0, r.y0);
    assert!(m.section == SEC_HOME && !m.searching);
    press(&mut m, "o");
    assert!(m.opt_open && m.query.is_empty());
    let seq = m.typed_seq;
    m.update(Msg::Typed(seq));
    assert_eq!(m.section, SEC_HOME, "the search does not pull back");
}

/// Up from the first search result goes back into the box; arriving at
/// Search, a digit goes on to its section and a letter starts a search.
#[test]
fn search_box() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.section = SEC_SEARCH;
    fill(
        m.cur_mut(),
        Message {
            shelves: vec![Shelf {
                title: "Songs".into(),
                tracks: vec![track("1", "a"), track("2", "b")],
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    m.move_sel("down");
    m.move_sel("up");
    assert!(!m.searching);
    m.move_sel("up");
    assert!(m.searching, "up from the first result reaches the box");

    let mut m = model(120, 35);
    type_in(&mut m, "81");
    assert!(m.section == SEC_HOME && m.query.is_empty() && !m.searching);
    type_in(&mut m, "8d");
    assert!(m.section == SEC_SEARCH && m.searching && m.query == "d");
    type_in(&mut m, "3");
    assert_eq!(m.query, "d3", "a digit after words types");
    m.query.clear();
    type_in(&mut m, "4");
    assert!(
        m.section == SEC_ALBUMS && !m.searching,
        "a digit into an empty box is its section"
    );
    type_in(&mut m, "8");
    press(&mut m, "enter");
    assert!(m.searching, "enter with no results goes into the box");

    // Searching again for what the results show goes back to them.
    let mut m = model(120, 35);
    m.query = "daft punk".into();
    m.run_search(true);
    m.stacks[SEC_SEARCH][0].loading = false;
    let v = m.view("Discovery", "cat:album:1", None);
    m.stacks[SEC_SEARCH].push(v);
    m.run_search(true);
    assert!(m.stack().len() == 1 && m.cur().key == "search:daft punk");
}

/// Both Apple and other providers resolve pasted links behind IPC.
#[tokio::test]
async fn paste_opens_links() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let (client, _events) = ipc::Client::new(a);
    let server = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let mut stream = BufReader::new(b);
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        let request: ipc::Request = serde_json::from_str(&line).unwrap();
        assert_eq!(request.cmd, ipc::CMD_RESOLVE);
        assert_eq!(request.query, "https://example.org/music/track");
        stream
            .get_mut()
            .write_all(&ipc::encode(&Message {
                id: request.id,
                items: vec![Item {
                    kind: KIND_ALBUM.into(),
                    id: "resolved".into(),
                    ..Default::default()
                }],
                ids: vec!["song".into()],
                ..Default::default()
            }))
            .await
            .unwrap();
    });
    let mut m = model(120, 35);
    m.client = Some(client);
    m.update(Msg::Paste("https://example.org/music/track\n".into()));
    assert_eq!(m.section, SEC_SEARCH);
    let task = m.cmds.pop().unwrap();
    let Cmd::Task(task) = task else {
        panic!("link was not delegated to the provider")
    };
    let result = match task.await {
        Some(Msg::Session(_, msg)) => Some(*msg),
        other => other,
    };
    assert!(matches!(result, Some(Msg::Open(it,song)) if it.id == "resolved" && song == "song"));
    server.await.unwrap();
    m.update(Msg::Paste("daft punk".into()));
    assert_eq!(m.query, "daft punk");
}

/// / filters the list where you are; esc brings it all back; tab takes
/// the words to all of Apple Music; another section leaves the box.
#[test]
fn filter_here() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.section = SEC_PLAYLISTS;
    let it = Item {
        kind: KIND_PLAYLIST.into(),
        id: "p.1".into(),
        name: "Mix".into(),
        ..Default::default()
    };
    let mut v = m.view("Mix", &it.key(), Some(it));
    fill(
        &mut v,
        tracks(vec![
            Track {
                artist: "Daft Punk".into(),
                ..track("1", "Around the World")
            },
            Track {
                artist: "Daft Punk".into(),
                ..track("2", "Get Lucky")
            },
            Track {
                artist: "New Order".into(),
                ..track("3", "Blue Monday")
            },
        ]),
    );
    m.stacks[SEC_PLAYLISTS].push(v);
    m.key("/");
    assert!(m.filtering && m.section == SEC_PLAYLISTS);
    type_in(&mut m, "daft luck");
    assert_eq!(m.cur().rows.len(), 1);
    assert_eq!(m.cur().rows[0].track.as_ref().unwrap().id, "2");
    assert!(shows(&mut m, "daft luck"), "the filter box shows");
    press(&mut m, "esc");
    assert!(m.cur().rows.len() == 3 && !m.filtering);
    assert_eq!(m.cur().sel, 1, "the row the filter showed stays selected");
    m.key("/");
    type_in(&mut m, "kraftwerk");
    press(&mut m, "tab");
    assert!(m.section == SEC_SEARCH && m.query == "kraftwerk");

    let mut m = model(120, 35);
    m.section = SEC_SONGS;
    fill(m.cur_mut(), tracks(vec![track("1", "A"), track("2", "B")]));
    m.key("/");
    m.switch_to(SEC_ALBUMS);
    assert!(
        !m.filtering,
        "the filter box does not follow to another section"
    );
}

/// Enter shows the song as playing at once; older reports do not take it
/// back; the player's own report of it ends the stand-in.
#[test]
fn play_shows_at_once() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    (m.state.player.id, m.state.player.title) = ("old".into(), "Old Song".into());
    fill(
        m.cur_mut(),
        tracks(vec![Track {
            artist: "A".into(),
            duration: 200.0,
            ..track("new", "New Song")
        }]),
    );
    m.cmds.clear();
    m.activate();
    let p = &m.state.player;
    assert!(
        p.title == "New Song" && p.playing && p.pos == 0.0,
        "the stage shows {p:?}"
    );
    assert!(m.cmds.iter().any(|c| matches!(c, Cmd::Task(_))) || m.client.is_none());
    let report = |m: &mut Model, id: &str, pos: f64, vol: f64| {
        let mut st = ready();
        (
            st.player.id,
            st.player.pos,
            st.player.volume,
            st.player.playing,
        ) = (id.into(), pos, vol, true);
        m.event(Message {
            state: Some(st),
            ..Default::default()
        });
    };
    report(&mut m, "old", 0.0, 0.5);
    assert!(
        m.state.player.title == "New Song" && m.state.player.volume == 0.5,
        "an older report took the song back"
    );
    report(&mut m, "new", 1.2, 0.5);
    assert!(
        m.pending.is_none() && m.state.player.pos == 1.2,
        "the player's own report takes over"
    );
}

/// Pause shows at once and older reports do not take it back.
#[test]
fn pause_shows_at_once() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    m.state.player.playing = true;
    m.key("n"); // no queue known: nothing to show yet
    assert!(m.state.player.playing && m.state.player.id == "1");
    m.want_play = None;
    m.toggle();
    assert!(!m.state.player.playing, "pause shows at once");
    let report = |m: &mut Model, playing: bool| {
        let mut st = m.state.clone();
        st.player.playing = playing;
        m.event(Message {
            state: Some(st),
            ..Default::default()
        });
    };
    report(&mut m, true);
    assert!(
        !m.state.player.playing,
        "an older report took the pause back"
    );
    report(&mut m, false);
    assert!(m.want_play.is_none());
    report(&mut m, true);
    assert!(m.state.player.playing, "a later play is not held back");
}

/// n shows the next song of the queue at once, and again for a second n;
/// a jump shows its song; the queue counts from the song on the stage.
#[test]
fn next_shows_at_once() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    (
        m.state.player.playing,
        m.state.player.index,
        m.state.player.length,
    ) = (true, 4, 20);
    m.next_seq = 1;
    let q = album_queue(&m, 5);
    m.got_next(1, q.clone(), 4, true);
    m.key("n");
    assert_eq!(
        (m.state.player.id.as_str(), m.state.player.index),
        (q[1].id.as_str(), 5)
    );
    m.key("n");
    assert_eq!(
        (m.state.player.id.as_str(), m.state.player.index),
        (q[2].id.as_str(), 6)
    );
    m.next_seq = 2;
    m.got_next(2, q.clone(), 4, true);
    assert_eq!(m.up_pos, 6);
    assert_eq!(m.up_tracks[0].id, q[2].id);
    m.jump(8);
    assert_eq!(
        (m.state.player.id.as_str(), m.state.player.index),
        (q[4].id.as_str(), 8)
    );
}

/// p after 3 s goes back to the start; seeking to the end goes on.
#[test]
fn prev_and_seek() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    m.state.player.pos = 42.0;
    m.key("p");
    assert_eq!(m.position(), 0.0);

    let mut m = playing(120, 35);
    m.next_seq = 1;
    let q = album_queue(&m, 2);
    m.got_next(1, q.clone(), 0, true);
    m.seek(1000.0);
    let seq = m.seek_seq;
    m.update(Msg::Seek(seq));
    assert_eq!(
        m.state.player.id, q[1].id,
        "seeking to the end shows the next song"
    );
}

/// Repeated seek keys collect into one request.
#[test]
fn seek_keys_collect() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    m.state.player.pos = 100.0;
    for _ in 0..3 {
        press(&mut m, "shift+right");
    }
    assert!(
        (m.position() - 130.0).abs() < 0.5,
        "the bar shows the target: {}",
        m.position()
    );
    let stale = m.seek_seq - 1;
    m.cmds.clear();
    m.update(Msg::Seek(stale));
    assert!(
        m.cmds.iter().all(|c| !matches!(c, Cmd::Task(_))),
        "an earlier key sends nothing"
    );
}

/// Fullscreen: tab steps, v lists the styles, a stops the change, and the
/// buttons on the bottom line do the same when clicked.
#[test]
fn viz_controls() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.toggle_full();
    let start = m.viz_style;
    m.full_key("tab");
    assert_ne!(m.viz_style, start);
    m.full_key("v");
    assert!(m.viz_list && shows(&mut m, "styles"));
    let from = m.viz_style;
    m.full_key("down");
    assert_ne!(m.viz_style, from, "moving in the list shows the style");
    m.full_key("esc");
    assert!(
        !m.viz_list && m.viz_style == from,
        "esc goes back to the style before the list"
    );
    m.full_key("v");
    m.full_key("3");
    assert!(!m.viz_list && m.viz_style == 2);
    m.full_key("a");
    assert!(m.opts.o.no_viz_cycle);
    screen(&mut m);
    let f = m
        .geo
        .foot
        .iter()
        .find(|f| f.action == "v")
        .expect("a styles button")
        .r;
    click(&mut m, f.x0, f.y0);
    assert!(m.viz_list, "clicking the styles button lists them");
    press(&mut m, "esc");
    press(&mut m, "f");
    assert!(!m.full, "f closes the visualizer");
}

/// The visualizer draws at 30 fps unless told otherwise; F steps through
/// 60, 120 and the screen's own rate.
#[test]
fn auto_fps() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.refresh = 144;
    m.full = true;
    for want in ["30 fps", "60 fps", "120 fps", "auto 144 fps", "30 fps"] {
        assert_eq!(m.fps_label(), want);
        m.full_key("F");
    }
    m.opts.o.viz_fps = 30;
    assert_eq!(m.draw_fps(), 30);
}

/// Nothing ticks while nothing moves; playing ticks for the clock and the
/// bear; the visualizer draws at its rate.
#[test]
fn next_tick() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.cur_mut().loaded = true;
    assert_eq!(m.next_tick(), None, "an idle, paused player does not tick");
    m.state.player.playing = true;
    let d = m.next_tick().expect("playing ticks");
    assert!(
        d >= Duration::from_millis(20) && d <= Duration::from_millis(505),
        "{d:?}"
    );
    (m.state.player.pos, m.state.player.dur, m.state_at) = (10.8, 100.0, Instant::now());
    assert!(
        m.next_tick().unwrap() <= Duration::from_millis(210),
        "the clock's next second is near"
    );
    m.marquee = true;
    assert_eq!(m.next_tick(), Some(FRAME_EVERY));
    m.marquee = false;
    m.blurred = true;
    assert_eq!(m.next_tick(), Some(Duration::from_secs(1)));
    m.state.player.playing = false;
    assert_eq!(m.next_tick(), None);
    (m.blurred, m.full, m.state.player.playing) = (false, true, true);
    assert_eq!(m.next_tick(), Some(Duration::from_secs(1) / 30));
    m.opts.o.viz_fps = 120;
    assert_eq!(m.next_tick(), Some(Duration::from_secs(1) / 120));
    m.state.player.playing = false;
    assert_eq!(m.next_tick(), Some(2 * FRAME_EVERY));

    // A stale tick, replaced by a sooner one, draws nothing.
    let seq = m.tick_seq;
    m.schedule(FRAME_EVERY);
    m.update(Msg::Tick(seq));
    assert!(m.still);
}

/// The spectrum streams only while the playing row's bars show in a
/// focused window, or the visualizer.
#[test]
fn spectrum_only_when_seen() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    m.state.player.playing = true;
    m.eq_shown = true;
    m.subscribe();
    assert!(m.spec_on);
    m.eq_shown = false;
    m.subscribe();
    assert!(!m.spec_on && m.spec.is_empty());
    m.eq_shown = true;
    m.blurred = true;
    m.subscribe();
    assert!(!m.spec_on, "no spectrum in the background");
    m.blurred = false;
    assert_eq!(
        m.next_tick(),
        Some(Duration::from_millis(125)),
        "the bars wobble by themselves"
    );
    m.full = true;
    m.subscribe();
    assert!(m.spec_on && m.spec_fps == VIZ_FPS);
}

/// The fullscreen visualizer draws on its own clock: spectrum frames in
/// between draw nothing.
#[test]
fn visualizer_draws_on_ticks() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(160, 45);
    m.state.player.playing = true;
    m.full = true;
    m.drawn = true;
    let seq = m.tick_seq;
    m.update(Msg::Tick(seq));
    assert!(m.tick_at.is_some(), "the visualizer ticks");
    for i in 0..10 {
        m.update(Msg::Event(Message {
            spectrum: (0..48).map(|b| (i * 37 + b * 11) % 256).collect(),
            ..Default::default()
        }));
        assert!(m.still, "a spectrum frame draws the visualizer");
    }
    let seq = m.tick_seq;
    m.update(Msg::Tick(seq));
    assert!(!m.still);
    let rows = screen(&mut m);
    assert!(
        rows[44].contains("Midnight City"),
        "the status line under it: {:?}",
        rows[44]
    );
}

/// A free-spinning wheel moves the selection at once but draws a frame at
/// a time, and covers load once it rests.
#[test]
fn wheel_spin() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(200, 55);
    m.section = SEC_SONGS;
    let ts = (0..600)
        .map(|i| Track {
            artwork: format!("cover{i}.jpg"),
            ..track(&i.to_string(), &format!("song {i}"))
        })
        .collect();
    fill(m.cur_mut(), tracks(ts));
    m.render();
    let mut draws = 0;
    let t0 = Instant::now();
    for _ in 0..500 {
        m.update(Msg::Mouse(Mouse {
            kind: MouseKind::WheelDown,
            button: Button::None,
            x: 10,
            y: 10,
        }));
        if !m.still {
            draws += 1;
        }
        m.render();
    }
    assert_eq!(m.cur().sel, 500);
    // At most a frame per WHEEL_EVERY (25 ms), however fast the notches.
    let most = t0.elapsed().as_millis() / 25 + 2;
    assert!(
        draws as u128 <= most && draws < 100,
        "500 notches drew {draws} times in {:?}",
        t0.elapsed()
    );
    assert!(m.fetching.is_empty(), "a spinning wheel fetches covers");
    m.update(Msg::Wheel);
    assert!(!m.still);
    assert!(m.render().iter().any(|l| text(l).contains("song 500")));
    m.wheel_at = Some(Instant::now() - Duration::from_millis(200));
    m.update(Msg::Wheel);
    assert!(
        !m.fetching.is_empty() && m.fetching.len() <= 32,
        "the rested wheel fetches {} covers",
        m.fetching.len()
    );
    assert!(!m.wheel_tick);
}

/// Covers stay for every row prefetch reaches: moving within them fetches
/// nothing again.
#[test]
fn covers_kept() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(200, 55);
    m.section = SEC_SONGS;
    let ts = (0..400)
        .map(|i| Track {
            artwork: format!("cover{i}.jpg"),
            ..track(&i.to_string(), &format!("song {i}"))
        })
        .collect();
    fill(m.cur_mut(), tracks(ts));
    let img = Arc::new(RgbaImage::new(8, 8));
    let deliver = |m: &mut Model| {
        let urls: Vec<String> = m.fetching.keys().cloned().collect();
        for url in &urls {
            m.update(Msg::Cover {
                url: url.clone(),
                img: Some(img.clone()),
            });
        }
        urls.len()
    };
    for _ in 0..8 {
        m.prefetch_cur();
        deliver(&mut m);
    }
    for i in 0..10 {
        m.move_sel("down");
        assert_eq!(deliver(&mut m), 0, "move {i} fetched covers again");
    }
}

/// A selected album lies on the playing cover as a card with its name;
/// it goes once the list rests, and the playing song brings none.
#[test]
fn card_on_cover() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(140, 40);
    m.state.player.playing = true;
    let blue = Item {
        kind: KIND_ALBUM.into(),
        id: "l.1".into(),
        name: "Blue".into(),
        artwork: "blue.jpg".into(),
        info: "1971".into(),
        ..Default::default()
    };
    let hejira = Item {
        kind: KIND_ALBUM.into(),
        id: "l.2".into(),
        name: "Hejira".into(),
        artwork: "hejira.jpg".into(),
        ..Default::default()
    };
    fill(
        m.cur_mut(),
        Message {
            items: vec![blue, hejira],
            ..Default::default()
        },
    );
    let size = Size {
        width: 40,
        height: 20,
    };
    let lines = m.stage_cover(size);
    assert_eq!(lines.len(), 20);
    assert!(
        lines.iter().all(|l| w(l) == 40),
        "every line keeps the cover's width"
    );
    assert!(
        text(&lines[19]).contains("Blue"),
        "a caption under the card: {:?}",
        text(&lines[19])
    );
    assert!(m.card_shown);
    m.frame = m.cur().sel_at + 41;
    m.card_shown = false;
    m.stage_cover(size);
    assert!(!m.card_shown, "the card goes after the list rested");
    m.move_sel("down");
    m.stage_cover(size);
    assert!(m.card_shown, "moving brings a card back");
    fill(
        m.cur_mut(),
        tracks(vec![Track {
            artwork: "x.jpg".into(),
            ..track("1", "Midnight City")
        }]),
    );
    m.card_shown = false;
    m.stage_cover(size);
    assert!(!m.card_shown, "the playing song shows no card of itself");

    // Reduce motion lays no card.
    m.opts.o.reduce_motion = true;
    m.move_sel("down");
    fill(
        m.cur_mut(),
        Message {
            items: vec![Item {
                kind: KIND_ALBUM.into(),
                id: "l.3".into(),
                name: "Court".into(),
                artwork: "c.jpg".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    m.stage_cover(size);
    assert!(!m.card_shown);
}

/// A cover that arrives is drawn as a card at once, and the rows next to
/// the selection are drawn ahead.
#[test]
fn card_drawn_at_once() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.schedule(Duration::from_secs(4));
    let size = Size {
        width: 20,
        height: 10,
    };
    m.thumb_want = ThumbReq {
        key: format!("{COVER_PIXEL}:a.jpg@20x10"),
        url: "a.jpg".into(),
        size,
    };
    m.update(Msg::Cover {
        url: "a.jpg".into(),
        img: Some(Arc::new(RgbaImage::new(10, 10))),
    });
    assert!(m.thumb_busy, "the card does not wait for the next tick");
    m.update(Msg::Thumb {
        key: format!("{COVER_PIXEL}:a.jpg@20x10"),
        lines: vec![plain("x")],
    });
    fill(
        m.cur_mut(),
        tracks(vec![
            Track {
                artwork: "a.jpg".into(),
                ..track("1", "")
            },
            Track {
                artwork: "b.jpg".into(),
                ..track("2", "")
            },
        ]),
    );
    m.card_size = size;
    m.covers
        .insert("b.jpg".into(), Arc::new(RgbaImage::new(10, 10)));
    assert_eq!(m.next_thumb().url, "b.jpg");
}

/// Long selected names scroll unless motion is reduced.
#[test]
fn reduce_motion_stops_scrolling() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.section = SEC_SONGS;
    fill(
        m.cur_mut(),
        tracks(vec![track("1", &"a very long name ".repeat(20))]),
    );
    m.render();
    assert!(m.marquee);
    m.opts.o.reduce_motion = true;
    m.render();
    assert!(!m.marquee);
}

/// Keytips keep every line inside the screen and put space over play.
#[test]
fn tips_layout() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    fill(
        m.cur_mut(),
        tracks(vec![Track {
            artist: "M83".into(),
            ..track("1", "Midnight City")
        }]),
    );
    m.opts.o.always_tips = true;
    widths(&mut m);
    let rows = screen(&mut m);
    let p = m.geo.play;
    let above = &rows[(p.y0 - 1) as usize];
    let col = above.chars().take(p.x0 as usize + 6).collect::<String>();
    assert!(
        col.ends_with("space "),
        "space is over the play button: {above:?} at {}",
        p.x0
    );
    let all = rows.join("\n");
    assert!(
        all.contains("z Z queue") && all.contains("1 Home"),
        "row keys and tab digits show"
    );
}

/// The menu shows its rows with the selected one's sentence, fits the
/// screen, and a click flips a row; options are saved here without a
/// daemon.
#[test]
fn options_menu() {
    let Some(env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(80, 24);
    m.opt_open = true;
    for (i, want) in [
        "Cover Colors",
        "Autoplay",
        "Reduce Motion",
        "Cover Style",
        "Show Shortcuts",
    ]
    .into_iter()
    .enumerate()
    {
        m.opt_sel = i;
        let rows = screen(&mut m);
        assert!(m.geo.options.x1 <= 80);
        let row = &rows[(m.geo.opt_row0 + m.opt_lines[i]) as usize];
        assert!(row.contains(&format!("▌  {want}")), "row {i} reads {row:?}");
        let hint = m.opt_hint(i);
        let boxed = rows[m.geo.options.y0 as usize..m.geo.options.y1 as usize].join("\n");
        assert!(boxed.contains(hint) && hint.ends_with('.'));
        assert!(boxed.contains("↑↓ move   space change   esc close"));
    }
    assert!(
        !m.opt_shown(super::options::OPT_BAR),
        "no bar row without omarchy"
    );
    let (x, y) = (
        m.geo.options.x0 + 3,
        m.geo.opt_row0 + m.opt_lines[OPT_MOTION],
    );
    click(&mut m, x, y);
    assert!(m.opts.o.reduce_motion && m.opt_sel == OPT_MOTION);
    let saved = std::fs::read_to_string(env.dir.join("config/brumm/options.json")).expect("saved");
    assert!(
        saved.contains("\"reduce_motion\": true") && saved.contains("\"no_bar_scroll\": true"),
        "{saved}"
    );
    m.options_key("esc");
    assert!(!m.opt_open);

    // → on, ← off, space flips.
    m.opt_open = true;
    m.opt_sel = OPT_MOTION;
    for (k, on) in [
        ("right", true),
        ("left", false),
        ("h", false),
        ("l", true),
        ("space", false),
    ] {
        m.options_key(k);
        assert_eq!(m.opts.o.reduce_motion, on, "{k}");
    }
    // The colors row is theme or cover, named by what it is.
    m.opt_sel = OPT_COLORS;
    m.options_key("left");
    assert!(m.opts.o.no_cover_colors && shows(&mut m, "Theme Colors") && shows(&mut m, "● theme"));
    m.options_key("right");
    assert!(!m.opts.o.no_cover_colors && shows(&mut m, "● cover"));
    // A switch shows at once behind the menu.
    m.opt_sel = OPT_KEYS;
    m.options_key("space");
    assert!(m.opts.o.always_tips && shows(&mut m, "1 Home"));
}

/// original is always a choice; without kitty graphics it draws smooth.
#[test]
fn cover_choices() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(100, 30);
    m.has_kitty = false;
    m.opt_open = true;
    m.opts.o.cover = COVER_ORIGINAL.into();
    assert_eq!(m.opts.cover(false), COVER_SMOOTH);
    assert!(shows(&mut m, "original"));
    m.opt_sel = OPT_COVER;
    assert!(shows(&mut m, "original needs kitty or Ghostty"));
    for want in [COVER_PIXEL, COVER_SMOOTH, COVER_ORIGINAL, COVER_PIXEL] {
        m.options_key("right");
        assert_eq!(m.opts.o.cover, want);
    }
    m.opts.o.cover = COVER_ORIGINAL.into();
    assert_eq!(m.opts.cover(true), COVER_ORIGINAL);
}

/// Any switch reduce motion took over starts it on, and is saved as
/// no_bar_scroll for the bar widget; switched off, that goes too.
#[test]
fn reduce_motion_migrates() {
    let Some(env) = testenv::isolated() else {
        return;
    };
    let path = env.dir.join("config/brumm/options.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for (old, on) in [
        (r#"{"no_scroll":true}"#, true),
        (r#"{"no_cards":true}"#, true),
        (r#"{"no_bar_scroll":true}"#, true),
        (r#"{"cover":"smooth"}"#, false),
    ] {
        std::fs::write(&path, old).unwrap();
        let mut m = model(100, 30);
        assert_eq!(m.opts.o.reduce_motion, on, "{old}");
        let b = std::fs::read_to_string(&path).unwrap().replace(' ', "");
        assert_eq!(
            b.contains(r#""no_bar_scroll":true"#),
            on,
            "{old}: saved {b}"
        );
        m.opt_sel = OPT_MOTION;
        m.options_key("space");
        let b = std::fs::read_to_string(&path).unwrap();
        assert!(
            super::options::Opts::load().o.reduce_motion != on
                && b.contains("no_bar_scroll") != on
                && !b.contains("no_scroll\""),
            "{old}: switched, saved {b}"
        );
    }
}

/// g goes over to the window, which brumm opens from then on; without one
/// it says so and stays.
#[test]
fn window_key() {
    let Some(env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(100, 30);
    m.key("g");
    assert!(
        m.flash.contains("not installed") && m.opts.o.start != crate::launch::GUI,
        "no window: {:?}",
        m.flash
    );
    use std::os::unix::fs::PermissionsExt;
    let gui = env.dir.join("bin").join(crate::launch::GUI_NAME);
    std::fs::write(&gui, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&gui, std::fs::Permissions::from_mode(0o755)).unwrap();
    m.cmds.clear();
    m.key("g");
    assert!(
        m.opts.o.start == crate::launch::GUI && !m.cmds.is_empty(),
        "a window: start {:?}",
        m.opts.o.start
    );
}

/// ! opens the feedback box; what is typed shows; esc closes it.
#[test]
fn feedback_box() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(100, 30);
    m.key("!");
    assert!(m.fb.is_some());
    type_in(&mut m, "no sound");
    assert_eq!(m.fb.as_ref().unwrap().input, "no sound");
    assert!(shows(&mut m, "no sound") && shows(&mut m, "open on GitHub"));
    press(&mut m, "esc");
    assert!(m.fb.is_none());
}

/// U always answers: checking, then the newest already or one to install;
/// once installed, it offers to restart.
#[test]
fn update_popup() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.upd = Some(UpdatePopup {
        checking: true,
        ..Default::default()
    });
    assert!(shows(&mut m, "looking for a new release"));
    m.update_checked(Ok(Message {
        version: "v0.5.2".into(),
        link: "v0.5.2".into(),
        ..Default::default()
    }));
    assert!(shows(&mut m, "v0.5.2 is the newest version"));
    m.update_key("esc");
    assert!(m.upd.is_none());
    m.upd = Some(UpdatePopup::default());
    let notes = vec![
        "# ʕ•ᴥ•ʔ brumm 0.5.3".to_string(),
        "🪟 brumm comes with a GUI. Press g to switch.".to_string(),
    ];
    m.update_checked(Ok(Message {
        version: "v0.5.2".into(),
        link: "v0.5.3".into(),
        pos: 1,
        notes,
        ..Default::default()
    }));
    assert!(
        shows(&mut m, "v0.5.3 is available")
            && shows(&mut m, "install")
            && shows(&mut m, "Press g to switch")
    );

    m.upd = Some(UpdatePopup {
        latest: "v9.9.9".into(),
        newer: true,
        installing: true,
        ..Default::default()
    });
    m.update_installed(Ok(()));
    assert!(m.upd.as_ref().is_some_and(|u| u.installed));
    let b: Vec<String> = m.update_box().iter().map(text).collect();
    assert!(
        b.iter().any(|l| l.contains("v9.9.9 is installed"))
            && b.iter().any(|l| l.contains("restart now"))
    );
    m.update_key("x");
    assert!(m.upd.is_some(), "another key does not close the offer");
    m.update_key("esc");
    assert!(m.upd.is_none());

    // Without a daemon U says so instead of hanging.
    m.check_update();
    assert!(
        m.upd
            .as_ref()
            .is_some_and(|u| !u.checking && !u.err.is_empty())
    );
}

/// The first start asks once about the bar widget, after signing in; the
/// answer is kept.
#[test]
fn bar_offer() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = Model::new(
        None,
        State {
            status: ipc::STATUS_LOGGED_OUT.into(),
            ..Default::default()
        },
    );
    (m.width, m.height) = (120, 35);
    m.has_omarchy = true;
    m.bar_read(Some(false));
    assert!(!m.bar_asking(), "asked while signed out");
    m.state.status = ipc::STATUS_READY.into();
    assert!(shows(&mut m, "Show brumm in the Omarchy bar?"));
    m.bar_key("x");
    assert!(m.bar_asking(), "another key answered");
    m.cmds.clear();
    m.bar_key("enter");
    assert!(
        m.bar_on && !m.bar_asking() && m.cmds.len() == 1,
        "yes switches the widget on"
    );
    m.bar_set(true, Some("refused".into()));
    assert!(
        !m.bar_on && m.flash.contains("refused"),
        "a refusal takes the switch back"
    );
    let mut m = model(120, 35); // the next start
    m.has_omarchy = true;
    m.bar_read(Some(false));
    assert!(!m.bar_asking(), "asked again");
}

/// A place left by one player opens the other right there, instead of
/// going to what plays.
#[test]
fn place_round_trip() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut a = model(120, 35);
    a.section = SEC_ALBUMS;
    a.player_view = true;
    let album = Item {
        kind: KIND_ALBUM.into(),
        id: "l.1".into(),
        name: "Discovery".into(),
        ..Default::default()
    };
    fill(
        a.cur_mut(),
        Message {
            items: vec![album.clone()],
            ..Default::default()
        },
    );
    let mut v = a.view("Discovery", &album.key(), Some(album.clone()));
    let songs = tracks(vec![
        track("s1", "One More Time"),
        track("s2", "Aerodynamic"),
    ]);
    fill(&mut v, songs.clone());
    v.sel = 1;
    a.stacks[SEC_ALBUMS].push(v);
    a.query = "daft punk".into();
    let p = serde_json::to_value(a.here()).unwrap();
    assert_eq!(p["sectionName"], "Albums");
    assert_eq!(p["sel"]["id"], "s2");

    let mut m = model(120, 35);
    m.state.player.source = "lib:playlist:p.1".into();
    m.placing = true;
    m.maybe_resume();
    assert!(!m.resumed, "resumed before the place answered");
    m.got_place(Some(p));
    assert!(
        m.section == SEC_ALBUMS
            && m.stack().len() == 2
            && m.cur().key == album.key()
            && m.resumed
            && !m.placing
    );
    assert!(m.query == "daft punk" && m.stacks[SEC_SEARCH][0].key == "search:daft punk");
    let id = m.cur().id;
    m.update(Msg::Loaded {
        view: id,
        reply: Ok(songs),
        select: String::new(),
    });
    assert!(m.player_view, "Now Playing mode kept");
    assert_eq!(m.cur().sel, 1);

    let mut m = model(120, 35);
    m.placing = true;
    m.got_place(None);
    assert!(!m.placing && m.section == SEC_HOME, "no place: as ever");
}

/// Going between an artist and an album goes back to the one already
/// open; a song asked for before the rows came is selected once they do.
#[test]
fn crumbs_do_not_repeat() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.section = SEC_ALBUMS;
    let album = Item {
        kind: KIND_ALBUM.into(),
        id: "l.1".into(),
        name: "Discovery".into(),
        ..Default::default()
    };
    let artist = Item {
        kind: KIND_ARTIST.into(),
        id: "ar.1".into(),
        name: "Daft Punk".into(),
        catalog: true,
        ..Default::default()
    };
    let v = m.view("Discovery", &album.key(), Some(album.clone()));
    m.push(v, "s2");
    assert_eq!(m.cur().want, "s2");
    let id = m.cur().id;
    m.update(Msg::Loaded {
        view: id,
        reply: Ok(tracks(vec![track("s1", "a"), track("s2", "b")])),
        select: String::new(),
    });
    assert_eq!(m.cur().sel, 1, "the song asked for is selected once loaded");
    let v = m.view("Daft Punk", &artist.key(), Some(artist));
    m.push(v, "");
    let v = m.view("Discovery", &album.key(), Some(album));
    m.push(v, "");
    assert_eq!(m.stack().len(), 2, "back to the album, not a third level");
    assert_eq!(m.cur().sel, 1, "its rows and selection kept");
}

/// Explore shows a grid of cards; a click on one opens its row.
#[test]
fn discovery_cards() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(140, 40);
    m.switch_to(SEC_EXPLORE);
    let items = (0..6)
        .map(|i| Item {
            kind: KIND_PLAYLIST.into(),
            id: format!("pl.{i}"),
            name: format!("Mix {i}"),
            catalog: true,
            ..Default::default()
        })
        .collect();
    fill(
        m.cur_mut(),
        Message {
            shelves: vec![Shelf {
                title: "Charts".into(),
                items,
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    assert!(m.discovering());
    widths(&mut m);
    assert!(m.geo.cards.len() >= 4, "{} cards", m.geo.cards.len());
    let c = m.geo.cards[2].clone();
    assert!(text(&m.render()[(c.r.y0) as usize]).contains("Mix 2"));
    click(&mut m, c.r.x0 + 1, c.r.y0);
    assert_eq!(m.stack().len(), 2);
    assert_eq!(m.cur().title, "Mix 2");
    assert!(!m.discovering(), "a playlist is a list");

    // Late artwork groups merge without losing the selection.
    m.back();
    let id = m.cur().id;
    m.load_explore_visuals(id, crate::music::ExplorePage::default());
    let v = m.view_by_id(id).unwrap();
    v.explore = Some(ExploreLoad {
        id: 7,
        page: crate::music::ExplorePage {
            items: vec![Item {
                kind: KIND_PLAYLIST.into(),
                id: "pl.2".into(),
                name: "Mix 2".into(),
                catalog: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        groups: vec![Vec::new()],
    });
    let shelves = vec![Shelf {
        title: "New".into(),
        items: vec![Item {
            kind: KIND_ALBUM.into(),
            id: "al.9".into(),
            name: "Fresh".into(),
            artwork: "f.jpg".into(),
            ..Default::default()
        }],
        ..Default::default()
    }];
    m.update(Msg::ExploreGroup {
        view: id,
        load: 7,
        group: 0,
        shelves,
    });
    let v = m.cur();
    assert!(v.rows.iter().any(|r| r.head == "New") && v.rows.iter().any(|r| r.head == "Browse"));
    assert_eq!(
        v.rows[v.sel].item.as_ref().unwrap().name,
        "Mix 2",
        "the selection stays on its row"
    );
}

/// The pointer is a hand over buttons and an arrow over the divider; a
/// bare move draws nothing.
#[test]
fn pointer_shapes() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(160, 45);
    m.render();
    let (p, d) = (m.geo.play, m.geo.divider);
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Moved,
        button: Button::None,
        x: p.x0,
        y: p.y0,
    }));
    assert!(m.still && m.pointer == super::pointer::SHAPE_POINTER);
    assert!(
        m.cmds
            .iter()
            .any(|c| matches!(c, Cmd::Raw(s) if s == "\x1b]22;pointer\x07"))
    );
    m.cmds.clear();
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Moved,
        button: Button::None,
        x: p.x0,
        y: p.y0,
    }));
    assert!(m.cmds.is_empty(), "the same shape is not sent again");
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Moved,
        button: Button::None,
        x: d.x0 + 1,
        y: d.y0 + 1,
    }));
    assert_eq!(m.pointer, super::pointer::SHAPE_RESIZE);
    m.update(Msg::Mouse(Mouse {
        kind: MouseKind::Moved,
        button: Button::None,
        x: 0,
        y: BODY_TOP + 10,
    }));
    assert_eq!(m.pointer, super::pointer::SHAPE_DEFAULT);
    assert_eq!(m.pointer_reset(), "", "the default needs no reset");
    m.pointer = "pointer".into();
    assert_eq!(m.pointer_reset(), "\x1b]22;default\x07");
}

/// The transport is where it is drawn: clicks pause, shuffle and seek.
#[test]
fn transport_clicks() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(160, 45);
    m.state.player.playing = true;
    m.render();
    let (play, shuffle, bar) = (m.geo.play, m.geo.shuffle, m.geo.bar);
    click(&mut m, play.x0 + 1, play.y0);
    assert!(!m.state.player.playing, "the play button pauses");
    click(&mut m, shuffle.x0, shuffle.y0);
    assert!(m.state.player.shuffle);
    click(&mut m, (bar.x0 + bar.x1) / 2, bar.y0);
    assert!(
        (m.position() - 120.0).abs() < 10.0,
        "a click mid-bar seeks to the middle: {}",
        m.position()
    );
}

/// The daemon going away reconnects; options another player set apply.
#[test]
fn daemon_events() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = playing(120, 35);
    m.update(Msg::Closed);
    assert!(m.state.status == ipc::STATUS_STARTING && m.state.message == "reconnecting");
    let o = crate::config::Options {
        cover: COVER_SMOOTH.into(),
        always_tips: true,
        ..Default::default()
    };
    m.thumbs.insert("x".into(), Vec::new());
    m.update(Msg::Event(Message {
        options: Some(o),
        ..Default::default()
    }));
    assert!(
        m.opts.o.always_tips && m.thumbs.is_empty(),
        "another player's options apply, and covers draw again"
    );
    m.update(Msg::Event(Message {
        spectrum: vec![255; 48],
        ..Default::default()
    }));
    assert_eq!(m.spec.len(), 48);
    assert!(m.spec.iter().all(|&s| s > 0.99));
    m.update(Msg::Event(Message {
        spectrum: vec![0; 48],
        ..Default::default()
    }));
    assert!(
        m.spec.iter().all(|&s| s > 0.3 && s < 1.0),
        "the bars fall back slowly"
    );
}

#[test]
fn provider_choice_skips_unavailable_services_and_blocks_normal_navigation() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut m = model(120, 35);
    m.state.status = ipc::STATUS_CHOOSE_PROVIDER.into();
    m.provider_choices = crate::provider::registry::choices();
    m.provider_choices[1].available = false; // Future adapters must remain unselectable.
    m.provider_key("down");
    assert_eq!(m.provider_index, 0, "unavailable adapters are skipped");
    m.provider_key("up");
    assert_eq!(m.provider_index, 0);
    let text = m
        .provider_box()
        .iter()
        .map(super::text::text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Apple Music"));
    assert!(text.contains("Tidal (coming soon)"));
}
