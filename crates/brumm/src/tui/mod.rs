//! The terminal player: connects to the daemon (starting it if needed),
//! and runs the screen until it closes. Closing it leaves the music
//! playing.
//!
//! One thread owns the model and the screen. Keys, the mouse and the
//! daemon's events come to it over a channel; what the model asks for
//! (requests, timers, drawing a cover) runs on the runtime and comes back
//! as a message. A frame is drawn once the messages waiting are handled,
//! and only when one of them changed something: the screen's buffer diff
//! then sends only the cells that changed.

mod accent;
mod bar;
mod browse;
mod card;
mod cover;
mod discovery;
mod features;
mod feedback;
mod filter;
mod geometry;
mod keys;
mod model;
mod options;
mod picker;
mod place;
mod player;
mod pointer;
mod text;
mod update;
mod upnext;
mod view;
mod viz;

#[cfg(test)]
mod tests;

use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::terminal::{
    self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen, SetTitle,
};
use ratatui::crossterm::{cursor, execute};

use crate::error::Result;
use crate::ipc::{self, Request};
use crate::launch;
use model::{Button, Cmd, Key, Model, Mouse, MouseKind, Msg};

/// The most frames a second the screen is drawn at: the fastest the
/// fullscreen visualizer is set to.
const MAX_FPS: u64 = options::MAX_DRAW_FPS as u64;

/// Connects to the daemon — starting it if needed — and runs the player.
pub fn run(rt: &tokio::runtime::Runtime) -> Result<()> {
    let (client, events) = rt.block_on(launch::connect())?;
    rt.spawn_blocking(|| crate::art::prune(&crate::config::cache_dir().join("covers"), 200 << 20));
    // Everything after this runs off the screen's thread; only this first
    // answer is waited for, and not forever.
    let subscribe = client.request(Request {
        cmd: ipc::CMD_SUBSCRIBE.into(),
        bands: model::BANDS,
        fps: model::METER_FPS,
        ..Default::default()
    });
    let reply = rt
        .block_on(async { tokio::time::timeout(Duration::from_secs(15), subscribe).await })
        .map_err(|_| {
            crate::error::err("the brumm daemon does not answer; see `journalctl --user -u brumm`")
        })??;
    let (tx, rx) = mpsc::channel::<Msg>();
    forward_events(rt, events, tx.clone());
    for sig in [
        tokio::signal::unix::SignalKind::terminate(),
        tokio::signal::unix::SignalKind::hangup(),
    ] {
        let tx = tx.clone();
        rt.spawn(async move {
            if let Ok(mut s) = tokio::signal::unix::signal(sig) {
                s.recv().await;
                let _ = tx.send(Msg::Quit);
            }
        });
    }

    let mut m = Model::new(Some(client.clone()), reply.state.unwrap_or_default());
    m.event(crate::ipc::Message {
        provider: reply.provider,
        ..Default::default()
    });
    m.tx = Some(tx.clone());
    let screen = Screen::enter()?;
    let _ = tx.send(Msg::Probe {
        bg: screen.bg,
        releases: screen.releases,
    });
    spawn_input(tx.clone());
    let (w, h) = terminal::size()?;
    m.update(Msg::Resize(w, h));
    m.init();

    let r = run_loop(rt, &mut m, &rx, &tx);
    // Images and the pointer shape are the terminal's to forget.
    let mut out = io::stdout();
    let _ = out.write_all(format!("{}{}", m.kitty_cleanup(), m.pointer_reset()).as_bytes());
    let _ = out.flush();
    drop(screen);
    r?;
    if m.reexec {
        // The binary changed under us (an update): continue in the new one.
        let self_path = launch::executable()?;
        rt.block_on(client.close());
        if let Some(c) = &m.client {
            rt.block_on(c.close());
        }
        let args: Vec<String> = std::env::args().skip(1).collect();
        let e = std::process::Command::new(self_path).args(args).exec();
        return Err(e.into());
    }
    Ok(())
}

/// Passes the daemon's events to the loop, and says when it went away.
pub(crate) fn forward_events(
    rt: &tokio::runtime::Runtime,
    mut events: tokio::sync::mpsc::Receiver<ipc::Message>,
    tx: Sender<Msg>,
) {
    rt.spawn(async move {
        while let Some(m) = events.recv().await {
            if tx.send(Msg::Event(m)).is_err() {
                return;
            }
        }
        let _ = tx.send(Msg::Closed);
    });
}

fn run_loop(
    rt: &tokio::runtime::Runtime,
    m: &mut Model,
    rx: &mpsc::Receiver<Msg>,
    tx: &Sender<Msg>,
) -> Result<()> {
    let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // Not term.clear(): since ratatui 0.30 it asks the terminal where the
    // cursor is, and the input thread, already reading, takes the answer.
    execute!(
        term.backend_mut(),
        terminal::Clear(terminal::ClearType::All)
    )?;
    let min_frame = Duration::from_nanos(1_000_000_000 / MAX_FPS);
    let mut dirty = true;
    let mut last_draw: Option<Instant> = None;
    loop {
        if run_cmds(rt, m, tx, term.backend_mut())? {
            return Ok(());
        }
        let now = Instant::now();
        let draw_at = last_draw.map_or(now, |l| l + min_frame);
        if dirty && now >= draw_at {
            // Publish each resize as a complete frame, so the terminal
            // never displays half of the old layout and half of the new.
            execute!(term.backend_mut(), BeginSynchronizedUpdate)?;
            let frame = term.draw(|f| m.draw(f.buffer_mut())).map(|_| ());
            let end = execute!(term.backend_mut(), EndSynchronizedUpdate);
            frame?;
            end?;
            last_draw = Some(now);
            dirty = false;
            continue; // drawing may have asked for covers
        }
        // Sleep until a message, the model's next frame of its own, or the
        // frame a change waits for.
        let mut wake = m.tick_at;
        if dirty {
            wake = Some(wake.map_or(draw_at, |t| t.min(draw_at)));
        }
        let msg = match wake {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        match msg {
            Ok(msg) => {
                dirty |= handle(m, msg);
                // What else is waiting is handled before the next frame.
                for _ in 0..256 {
                    let Ok(msg) = rx.try_recv() else { break };
                    dirty |= handle(m, msg);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if m.tick_at.is_some_and(|t| t <= Instant::now()) {
            let seq = m.tick_seq;
            dirty |= handle(m, Msg::Tick(seq));
        }
    }
}

/// Lets the model handle msg; true when the screen changed.
fn handle(m: &mut Model, msg: Msg) -> bool {
    m.update(msg);
    !m.still
}

/// Starts what the model asked for; true when it asked to quit.
fn run_cmds(
    rt: &tokio::runtime::Runtime,
    m: &mut Model,
    tx: &Sender<Msg>,
    out: &mut impl Write,
) -> io::Result<bool> {
    let mut raw = String::new();
    let mut quit = false;
    for c in std::mem::take(&mut m.cmds) {
        match c {
            Cmd::Task(f) => {
                let tx = tx.clone();
                rt.spawn(async move {
                    if let Some(msg) = f.await {
                        let _ = tx.send(msg);
                    }
                });
            }
            Cmd::Blocking(f) => {
                let tx = tx.clone();
                rt.spawn_blocking(move || {
                    if let Some(msg) = f() {
                        let _ = tx.send(msg);
                    }
                });
            }
            Cmd::After(d, msg) => {
                if d.is_zero() {
                    let _ = tx.send(*msg);
                    continue;
                }
                let tx = tx.clone();
                rt.spawn(async move {
                    tokio::time::sleep(d).await;
                    let _ = tx.send(*msg);
                });
            }
            Cmd::Raw(s) => raw.push_str(&s),
            Cmd::Quit => quit = true,
        }
    }
    if !raw.is_empty() {
        out.write_all(raw.as_bytes())?;
        out.flush()?;
    }
    Ok(quit)
}

/// Reads the terminal's input on a thread of its own.
fn spawn_input(tx: Sender<Msg>) {
    std::thread::Builder::new()
        .name("brumm-input".into())
        .spawn(move || {
            loop {
                let Ok(ev) = event::read() else { return };
                let Some(msg) = to_msg(ev) else { continue };
                if tx.send(msg).is_err() {
                    return;
                }
            }
        })
        .expect("start the input thread");
}

/// A terminal event as the model's message.
pub(crate) fn to_msg(ev: Event) -> Option<Msg> {
    Some(match ev {
        Event::Key(k) => Msg::Key(key(k)?),
        Event::Mouse(ms) => Msg::Mouse(mouse(ms)?),
        Event::Resize(w, h) => Msg::Resize(w, h),
        Event::FocusGained => Msg::Focus(true),
        Event::FocusLost => Msg::Focus(false),
        Event::Paste(s) => Msg::Paste(s),
    })
}

/// Names a key the way the player's handlers match it: "enter",
/// "shift+tab", "ctrl+c", "a", "A", "space".
pub(crate) fn key(k: KeyEvent) -> Option<Key> {
    let mods = k.modifiers;
    let (ctrl, alt, shift) = (
        mods.contains(KeyModifiers::CONTROL),
        mods.contains(KeyModifiers::ALT),
        mods.contains(KeyModifiers::SHIFT),
    );
    let mut text = String::new();
    let base = match k.code {
        KeyCode::Char(' ') if !ctrl && !alt => {
            text.push(' ');
            "space".to_string()
        }
        KeyCode::Char(c) => {
            let c = if shift && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            };
            if ctrl || alt {
                c.to_lowercase().to_string()
            } else {
                text.push(c);
                c.to_string()
            }
        }
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab if shift => return Some(named("shift+tab", k)),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => return Some(named("shift+tab", k)),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pgup".into(),
        KeyCode::PageDown => "pgdown".into(),
        KeyCode::F(n) => format!("f{n}"),
        _ => return None,
    };
    let mut name = String::new();
    if ctrl {
        name.push_str("ctrl+");
    }
    if alt {
        name.push_str("alt+");
    }
    // Shift is in the letter itself; for the other keys it is named.
    if shift && text.is_empty() && !matches!(k.code, KeyCode::Char(_)) {
        name.push_str("shift+");
    }
    name.push_str(&base);
    Some(Key {
        name,
        text,
        repeat: k.kind == KeyEventKind::Repeat,
        release: k.kind == KeyEventKind::Release,
    })
}

fn named(name: &str, k: KeyEvent) -> Key {
    Key {
        repeat: k.kind == KeyEventKind::Repeat,
        release: k.kind == KeyEventKind::Release,
        ..Key::named(name)
    }
}

pub(crate) fn mouse(ms: MouseEvent) -> Option<Mouse> {
    let b = |b: MouseButton| match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    };
    let (kind, button) = match ms.kind {
        MouseEventKind::Down(x) => (MouseKind::Down, b(x)),
        MouseEventKind::Up(x) => (MouseKind::Up, b(x)),
        MouseEventKind::Drag(x) => (MouseKind::Moved, b(x)),
        MouseEventKind::Moved => (MouseKind::Moved, Button::None),
        MouseEventKind::ScrollUp => (MouseKind::WheelUp, Button::None),
        MouseEventKind::ScrollDown => (MouseKind::WheelDown, Button::None),
        _ => return None,
    };
    Some(Mouse {
        kind,
        button,
        x: ms.column as i64,
        y: ms.row as i64,
    })
}

/// A terminal cell's height divided by its width in pixels, so the cover
/// can be drawn square. Terminals that do not report pixel sizes get the
/// common 2:1.
pub fn cell_aspect() -> f64 {
    // SAFETY: TIOCGWINSZ fills the winsize it is given and nothing else.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
    if !ok || ws.ws_col == 0 || ws.ws_row == 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 {
        return 2.0;
    }
    let a = (ws.ws_ypixel as f64 / ws.ws_row as f64) / (ws.ws_xpixel as f64 / ws.ws_col as f64);
    if (1.2..=3.5).contains(&a) { a } else { 2.0 }
}

/// The terminal in the player's modes: raw, the alternate screen, every
/// mouse move, focus, pastes and — where the terminal has them — key
/// releases (hold space to preview). Dropping it puts the terminal back.
struct Screen {
    keyboard: bool,
    bg: Option<[u8; 3]>,
    releases: bool,
}

impl Screen {
    fn enter() -> io::Result<Screen> {
        terminal::enable_raw_mode()?;
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore(true);
            hook(info);
        }));
        let (bg, keyboard) = probe();
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange,
            EnableBracketedPaste,
            SetTitle("brumm"),
            cursor::Hide
        )?;
        if keyboard {
            execute!(
                out,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
        }
        Ok(Screen {
            keyboard,
            bg,
            releases: keyboard,
        })
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        restore(self.keyboard);
    }
}

fn restore(keyboard: bool) {
    let mut out = io::stdout();
    if keyboard {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableBracketedPaste,
        DisableFocusChange,
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    );
    let _ = terminal::disable_raw_mode();
}

/// Asks the terminal for its background color and whether it speaks the
/// kitty keyboard protocol, before anything else reads its input: the
/// primary device attributes, which every terminal answers, end the wait.
fn probe() -> (Option<[u8; 3]>, bool) {
    let mut out = io::stdout();
    if out
        .write_all(b"\x1b]11;?\x1b\\\x1b[?u\x1b[c")
        .and_then(|_| out.flush())
        .is_err()
    {
        return (None, false);
    }
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut got: Vec<u8> = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut pfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd for the call's duration.
        let n = unsafe { libc::poll(&mut pfd, 1, left.as_millis().max(1) as i32) };
        if n <= 0 {
            break;
        }
        let mut buf = [0u8; 256];
        // SAFETY: reads into a buffer of the given length.
        let r = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
        if r <= 0 {
            break;
        }
        got.extend_from_slice(&buf[..r as usize]);
        if parse_probe(&got).2 {
            break;
        }
    }
    let (bg, kitty, _) = parse_probe(&got);
    (bg, kitty)
}

/// Reads the probe's answers: the background (OSC 11), whether a kitty
/// keyboard answer came (CSI ? flags u), and whether the device attributes
/// (CSI ? … c) ended it.
pub(crate) fn parse_probe(b: &[u8]) -> (Option<[u8; 3]>, bool, bool) {
    let s = String::from_utf8_lossy(b);
    let bg = s.find("]11;rgb:").and_then(|i| {
        let rest = &s[i + 8..];
        let end = rest.find(['\x07', '\x1b']).unwrap_or(rest.len());
        let parts: Vec<&str> = rest[..end].split('/').collect();
        if parts.len() != 3 {
            return None;
        }
        let mut c = [0u8; 3];
        for (k, p) in parts.iter().enumerate() {
            let v = u32::from_str_radix(p, 16).ok()?;
            let max = (1u32 << (4 * p.len().clamp(1, 4))) - 1;
            c[k] = (v * 255 / max) as u8;
        }
        Some(c)
    });
    let (mut kitty, mut done) = (false, false);
    let mut rest = s.as_ref();
    while let Some(i) = rest.find("\x1b[?") {
        let tail = &rest[i + 3..];
        let end = tail
            .find(|c: char| !(c.is_ascii_digit() || c == ';'))
            .unwrap_or(tail.len());
        match tail[end..].chars().next() {
            Some('u') => kitty = true,
            Some('c') => done = true,
            _ => {}
        }
        rest = &tail[end..];
    }
    (bg, kitty, done)
}
