//! The built program end to end: `brumm daemon` in a private environment
//! (no credentials, no session bus, no Omarchy, no network), spoken to
//! over its socket as the players and the window speak to it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Env {
    root: PathBuf,
    daemon: Option<Child>,
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(mut d) = self.daemon.take() {
            let _ = d.kill();
            let _ = d.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn env(name: &str) -> Env {
    let root = std::env::temp_dir().join(format!("brumm-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["run", "config", "cache", "data", "bin", "home"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::set_permissions(root.join("run"), std::fs::Permissions::from_mode(0o710)).unwrap();
    Env { root, daemon: None }
}

fn brumm(root: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_brumm"));
    c.env_clear()
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env("XDG_RUNTIME_DIR", root.join("run"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("PATH", root.join("bin"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", root.join("no-bus").display()),
        );
    c
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} did not appear",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
    next: i64,
}

impl Conn {
    fn open(sock: &Path) -> Conn {
        let w = UnixStream::connect(sock).unwrap();
        w.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Conn {
            r: BufReader::new(w.try_clone().unwrap()),
            w,
            next: 0,
        }
    }

    /// Sends req and returns its answer, skipping pushed messages.
    fn ask(&mut self, mut req: Value) -> Value {
        self.next += 1;
        req["id"] = json!(self.next);
        let mut line = serde_json::to_vec(&req).unwrap();
        line.push(b'\n');
        self.w.write_all(&line).unwrap();
        loop {
            let mut l = String::new();
            assert!(self.r.read_line(&mut l).unwrap() > 0, "the daemon hung up");
            let m: Value = serde_json::from_str(&l).unwrap();
            if m["id"] == json!(self.next) {
                return m;
            }
        }
    }
}

#[test]
fn version() {
    let e = env("version");
    let out = brumm(&e.root).arg("version").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("brumm "));
    let bad = brumm(&e.root).arg("nonsense").output().unwrap();
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn daemon_answers_without_credentials_and_quits() {
    let mut e = env("daemon");
    let sock = e.root.join("run/brumm.sock");
    e.daemon = Some(
        brumm(&e.root)
            .arg("daemon")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(&sock);
    assert_eq!(
        std::fs::metadata(e.root.join("run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o710,
        "the daemon must preserve its caller's runtime directory permissions"
    );
    assert_eq!(
        std::fs::metadata(e.root.join("cache/brumm"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "the application cache must be private from the first start"
    );
    assert_eq!(
        std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777,
        0o600,
        "the socket is the user's alone"
    );

    // A second daemon refuses to start next to the first.
    let second = brumm(&e.root).arg("daemon").output().unwrap();
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("already running"));

    let mut c = Conn::open(&sock);
    let unsupported = c.ask(json!({"cmd":"provider","protocolVersion":99}));
    assert!(
        unsupported["error"]
            .as_str()
            .unwrap()
            .contains("unsupported socket protocol")
    );
    let supported = c.ask(json!({"cmd":"provider","protocolVersion":1}));
    assert_eq!(supported["provider"]["protocolVersion"], 1);
    let sub = c.ask(json!({"cmd": "subscribe"}));
    let st = &sub["state"];
    assert!(st["status"].is_string(), "{sub}");

    // Options: the defaults, then a change every subscriber hears of.
    let o = c.ask(json!({"cmd": "options"}));
    assert_eq!(o["options"], json!({}), "{o}");
    let mut other = Conn::open(&sock);
    other.ask(json!({"cmd": "subscribe"}));
    let set = c.ask(json!({"cmd": "options", "options": {"start": "gui", "viz_fps": 60}}));
    assert_eq!(set["options"], json!({"start": "gui", "viz_fps": 60}));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(e.root.join("config/brumm/options.json")).unwrap())
            .unwrap();
    assert_eq!(saved["start"], "gui");
    let mut heard = false;
    for _ in 0..10 {
        let mut l = String::new();
        if other.r.read_line(&mut l).is_err() {
            break;
        }
        if serde_json::from_str::<Value>(&l).unwrap()["options"]["start"] == "gui" {
            heard = true;
            break;
        }
    }
    assert!(heard, "the other player did not hear of the change");
    let bad = c.ask(json!({"cmd": "options", "options": {"viz_fps": "fast"}}));
    assert!(
        bad["error"].as_str().unwrap().starts_with("options:"),
        "{bad}"
    );

    // Place: kept as it comes, cleared with null.
    c.ask(json!({"cmd": "place", "place": {"section": 3}}));
    assert_eq!(
        c.ask(json!({"cmd": "place"}))["place"],
        json!({"section": 3})
    );
    c.ask(json!({"cmd": "place", "place": null}));
    let cleared = c.ask(json!({"cmd": "place"}));
    assert!(cleared.as_object().unwrap().contains_key("place") && cleared["place"].is_null());

    // Without a session nothing reaches Apple, and says so.
    let list = c.ask(json!({"cmd": "list", "list": "albums"}));
    assert_eq!(list["error"], "still starting");
    let play = c.ask(json!({"cmd": "toggle"}));
    assert_eq!(play["error"], "the player is still starting");

    // A garbled line is skipped; the connection stays.
    c.w.write_all(b"{not json\n").unwrap();
    assert!(c.ask(json!({"cmd": "options"}))["options"].is_object());

    c.ask(json!({"cmd": "quit"}));
    let mut d = e.daemon.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = d.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(Instant::now() < deadline, "the daemon did not quit");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!sock.exists(), "the socket was left behind");
}

#[test]
fn provider_selection_persists_and_switches_without_reusing_stale_requests() {
    let mut e = env("provider-choice");
    e.daemon = Some(
        brumm(&e.root)
            .arg("daemon")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let sock = e.root.join("run/brumm.sock");
    wait_for(&sock);
    let mut c = Conn::open(&sock);
    let before = c.ask(json!({"cmd":"provider"}));
    assert_eq!(before["provider"]["needsChoice"], true);
    assert_eq!(before["provider"]["selected"], "");
    assert_eq!(before["provider"]["choices"][0]["id"], "apple");
    assert_eq!(before["provider"]["choices"][1]["available"], true);
    let bad = c.ask(json!({"cmd":"provider","query":"unknown"}));
    assert!(
        bad["error"]
            .as_str()
            .unwrap()
            .contains("unknown music provider")
    );
    assert!(!e.root.join("config/brumm/config.json").exists());
    let mut other = Conn::open(&sock);
    other.ask(json!({"cmd":"subscribe"}));
    let chosen = c.ask(json!({"cmd":"provider","query":"apple"}));
    assert_eq!(chosen["provider"]["selected"], "apple");
    assert_eq!(chosen["provider"]["needsChoice"], false);
    loop {
        let mut line = String::new();
        assert!(other.r.read_line(&mut line).unwrap() > 0);
        let message: Value = serde_json::from_str(&line).unwrap();
        if message.get("provider").is_some() {
            assert_eq!(message["provider"], chosen["provider"]);
            break;
        }
    }
    let saved: Value =
        serde_json::from_slice(&std::fs::read(e.root.join("config/brumm/config.json")).unwrap())
            .unwrap();
    assert_eq!(saved["provider"], "apple");
    let after = c.ask(json!({"cmd":"provider"}));
    assert_eq!(after["provider"]["needsChoice"], false);
    let apple_epoch = after["provider"]["generation"].as_i64().unwrap();
    let tidal = c.ask(json!({"cmd":"provider","query":"tidal"}));
    assert_eq!(tidal["provider"]["selected"], "tidal");
    assert_eq!(tidal["provider"]["navigation"][5]["label"], "Tracks");
    let tidal_epoch = tidal["provider"]["generation"].as_i64().unwrap();
    assert!(tidal_epoch > apple_epoch);
    let stale = c.ask(json!({"cmd":"set-favorite","generation":apple_epoch,"saved":true,"refs":[{"kind":"song","id":"123"}]}));
    assert!(stale["error"].as_str().unwrap().contains("session"));
    let restored = c.ask(json!({"cmd":"provider","query":"apple"}));
    assert_eq!(restored["provider"]["selected"], "apple");
    assert!(restored["provider"]["generation"].as_i64().unwrap() > tidal_epoch);
    assert_eq!(restored["provider"]["navigation"][5]["label"], "Songs");
    c.ask(json!({"cmd":"quit"}));
    e.daemon.as_mut().unwrap().wait().unwrap();
}
