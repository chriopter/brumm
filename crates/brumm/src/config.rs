//! brumm's paths, credentials and the options its players share.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Result, err};

/// The Apple Music developer token (an ES256 JWT, valid at most six
/// months), embedded at build time:
///
/// ```sh
/// BRUMM_EMBED_DEV_TOKEN=<jwt> cargo build --release
/// ```
///
/// It is kept as one plain string in the program, so `dev/setup` can carry
/// it over from an installed release. A developer_token in the config
/// file takes precedence, so a local token can be rotated without a
/// rebuild.
pub const DEV_TOKEN: &str = match option_env!("BRUMM_EMBED_DEV_TOKEN") {
    Some(t) => t,
    None => "",
};

/// This build's version: a release tag (v1.2.3) or a development name.
pub const VERSION: &str = match option_env!("BRUMM_VERSION") {
    Some(v) => v,
    None => "dev",
};

pub fn home() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = std::env::var_os("BRUMM_TEST_HOME") {
        return root.into();
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => home().join(fallback),
    }
}

pub fn dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("brumm")
}

pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("brumm")
}

/// Account-derived browse and resume data is isolated from the selected service.
pub fn session_cache_dir() -> PathBuf {
    let cfg = Config::load().unwrap_or_default();
    session_cache_dir_for(&cfg)
}

pub fn session_cache_dir_for(cfg: &Config) -> PathBuf {
    match crate::provider::registry::definition(cfg.effective_provider()) {
        Ok(provider) => cache_dir()
            .join(provider.id())
            .join(if provider.signed_in(cfg) {
                provider.account_key(cfg)
            } else {
                "signed-out".into()
            }),
        _ => cache_dir(),
    }
}

/// A provider may expire persisted metadata instead of inheriting Apple's cache
/// lifetime. Also remove expired files from inactive accounts on startup.
pub fn expire_session_files() {
    for provider in crate::provider::registry::choices() {
        let Some(age) = crate::provider::registry::definition(&provider.id)
            .ok()
            .and_then(|p| p.cache_max_age())
        else {
            continue;
        };
        let Ok(accounts) = fs::read_dir(cache_dir().join(&provider.id)) else {
            continue;
        };
        for account in accounts.flatten() {
            for name in ["library.json", "resume.json"] {
                let path = account.path().join(name);
                if fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|elapsed| elapsed > age)
                {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

/// Import the original Apple cache once, leaving it available to the old build.
pub fn migrate_session_cache() -> Result<()> {
    let cfg = Config::load()?;
    // These files predate providers and contain Apple IDs exclusively.
    if cfg.effective_provider() != "apple" {
        return Ok(());
    }
    let provider = match crate::provider::registry::definition(cfg.effective_provider()) {
        Ok(p) if p.signed_in(&cfg) => p,
        _ => return Ok(()),
    };
    let marker = cache_dir().join(format!("legacy-{}-imported", provider.id()));
    if marker.exists() {
        return Ok(());
    }
    let target = session_cache_dir_for(&cfg);
    fs::create_dir_all(&target)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700))?;
    for name in ["library.json", "resume.json"] {
        if let Ok(bytes) = fs::read(cache_dir().join(name))
            && !target.join(name).exists()
        {
            write_private(&target.join(name), &bytes)?;
        }
    }
    write_private(&marker, b"1\n")?;
    Ok(())
}

/// The daemon's control socket.
pub fn socket() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d).join("brumm.sock"),
        // Not the shared /tmp, where another user could claim the name.
        _ => cache_dir().join("brumm.sock"),
    }
}

/// Writes data to path through a temporary file renamed over it, readable
/// by the owner alone.
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        // An older file of that name keeps its mode: make sure.
        f.set_permissions(fs::Permissions::from_mode(0o600))?;
        f.write_all(data)?;
    }
    fs::rename(&tmp, path)
}

/// The user's Apple Music session, and a developer token of their own.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(
        default,
        skip_serializing_if = "crate::provider::tidal::auth::Session::is_empty"
    )]
    pub tidal: crate::provider::tidal::auth::Session,
    /// The saved service; old Apple sessions remain compatible.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub developer_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spotify_client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spotify_refresh_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spotify_user: String,
}

fn config_path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    /// Read-modify-write without overwriting another task's refreshed credentials
    /// or selection. The closure must perform only short, synchronous work.
    pub fn update<T>(change: impl FnOnce(&mut Config) -> Result<T>) -> Result<T> {
        static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        let mut cfg = Self::load()?;
        let result = change(&mut cfg)?;
        cfg.save()?;
        Ok(result)
    }
    pub fn effective_provider(&self) -> &str {
        if self.provider.is_empty() && !self.user_token.is_empty() {
            "apple"
        } else {
            &self.provider
        }
    }
    pub fn needs_provider_choice(&self) -> bool {
        self.effective_provider().is_empty()
    }

    /// The config; a missing file is an empty config, not an error.
    pub fn load() -> Result<Config> {
        match fs::read(config_path()) {
            Ok(b) => Ok(serde_json::from_slice(&b)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the config with owner-only permissions: it holds the user's
    /// Apple Music session.
    pub fn save(&self) -> Result<()> {
        let d = dir();
        fs::create_dir_all(&d)?;
        // Made by something else first, it may be open to others.
        let _ = fs::set_permissions(&d, fs::Permissions::from_mode(0o700));
        let mut b = serde_json::to_vec_pretty(self)?;
        b.push(b'\n');
        write_private(&config_path(), &b)?;
        Ok(())
    }

    /// The effective developer token.
    pub fn developer(&self) -> &str {
        if self.developer_token.is_empty() {
            DEV_TOKEN
        } else {
            &self.developer_token
        }
    }

    /// When the developer token stops working, in Unix seconds.
    pub fn expires(&self) -> Option<i64> {
        token_expiry(self.developer())
    }
}

/// The expiry (exp claim) of a JWT, in Unix seconds.
pub fn token_expiry(jwt: &str) -> Option<i64> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let claims: Value = serde_json::from_slice(&raw).ok()?;
    match claims.get("exp")?.as_i64()? {
        0 => None,
        exp => Some(exp),
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(i: &i64) -> bool {
    *i == 0
}

/// The few switches in the players' options menu, each a plain flag. The
/// zero value is the default, so a missing file or field means "as
/// shipped". The daemon reads them too (autoplay).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Options {
    /// pixel (default), smooth or original
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cover: String,
    /// The theme's accents, not the cover's.
    #[serde(skip_serializing_if = "is_false")]
    pub no_cover_colors: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub no_autoplay: bool,
    /// Names stay put, no cover cards.
    #[serde(skip_serializing_if = "is_false")]
    pub reduce_motion: bool,
    /// Keys on their buttons.
    #[serde(skip_serializing_if = "is_false")]
    pub always_tips: bool,
    /// reduce_motion, as the bar widget reads it (BarWidget.qml).
    #[serde(skip_serializing_if = "is_false")]
    pub no_bar_scroll: bool,
    /// The visualizer keeps its style instead of moving on each minute.
    #[serde(skip_serializing_if = "is_false")]
    pub no_viz_cycle: bool,
    /// The visualizer's controls fade out while the mouse rests.
    #[serde(skip_serializing_if = "is_false")]
    pub viz_bare: bool,
    /// The visualizer's style by name, as last shown: f opens on it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub viz: String,
    /// Shared player presentation: true uses the large visualizer.
    #[serde(skip_serializing_if = "is_false")]
    pub viz_full: bool,
    /// Fullscreen frames per second: 30 (default), 60, 120, or -1 for the
    /// screen's rate.
    #[serde(skip_serializing_if = "is_zero")]
    pub viz_fps: i64,
    /// The bar widget as last set or seen: on or off; empty: never known.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bar: String,
    /// The first start asked about the bar widget.
    #[serde(skip_serializing_if = "is_false")]
    pub bar_offered: bool,
    /// What plain brumm opens: tui (default) or gui.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub start: String,
    /// SUPER+SHIFT+M (and +ALT) open brumm, not Omarchy's music apps.
    #[serde(skip_serializing_if = "is_false")]
    pub music_keys: bool,
    /// Updates to the development builds of main too: the vX.Y.Z-dev.N
    /// pre-releases, every change as soon as it is built.
    #[serde(skip_serializing_if = "is_false")]
    pub dev_builds: bool,
}

/// Switches reduce motion took over: any of them set starts it on.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Legacy {
    no_scroll: bool,
    no_cards: bool,
}

fn options_path() -> PathBuf {
    dir().join("options.json")
}

impl Options {
    /// The saved options; anything unreadable is the defaults.
    pub fn load() -> Options {
        let Ok(b) = fs::read(options_path()) else {
            return Options::default();
        };
        let mut o: Options = serde_json::from_slice(&b).unwrap_or_default();
        let l: Legacy = serde_json::from_slice(&b).unwrap_or_default();
        o.reduce_motion = o.reduce_motion || o.no_bar_scroll || l.no_scroll || l.no_cards;
        o
    }

    pub fn save(&self) -> Result<()> {
        let mut o = self.clone();
        o.no_bar_scroll = o.reduce_motion;
        let mut b = serde_json::to_vec_pretty(&o)?;
        b.push(b'\n');
        fs::create_dir_all(dir())?;
        write_private(&options_path(), &b)?;
        Ok(())
    }

    /// The options by their JSON names, every one, zero or not: what a
    /// player sends the daemon to set them all.
    pub fn fields(&self) -> Map<String, Value> {
        let o = self;
        let mut m = Map::new();
        m.insert("cover".into(), o.cover.clone().into());
        m.insert("no_cover_colors".into(), o.no_cover_colors.into());
        m.insert("no_autoplay".into(), o.no_autoplay.into());
        m.insert("reduce_motion".into(), o.reduce_motion.into());
        m.insert("always_tips".into(), o.always_tips.into());
        m.insert("no_bar_scroll".into(), o.no_bar_scroll.into());
        m.insert("no_viz_cycle".into(), o.no_viz_cycle.into());
        m.insert("viz".into(), o.viz.clone().into());
        m.insert("viz_full".into(), o.viz_full.into());
        m.insert("viz_bare".into(), o.viz_bare.into());
        m.insert("viz_fps".into(), o.viz_fps.into());
        m.insert("bar".into(), o.bar.clone().into());
        m.insert("bar_offered".into(), o.bar_offered.into());
        m.insert("start".into(), o.start.clone().into());
        m.insert("music_keys".into(), o.music_keys.into());
        m
    }

    /// These options with those named in set changed; a value of the
    /// wrong kind is an error, a null leaves its option as it is.
    pub fn with(&self, set: &Map<String, Value>) -> Result<Options> {
        let mut all = self.fields();
        for (k, v) in set {
            if !v.is_null() && all.contains_key(k) {
                all.insert(k.clone(), v.clone());
            }
        }
        serde_json::from_value(Value::Object(all)).map_err(|e| err(format!("options: {e}")))
    }
}

#[cfg(test)]
pub(crate) mod testenv {
    //! Filesystem tests run in a child test process whose environment is
    //! set before it starts. No test mutates the shared process environment.
    use std::path::PathBuf;
    use std::process::Command;

    pub struct Env {
        pub dir: PathBuf,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Runs this test in an isolated child, returning None in the parent.
    /// Call as `let Some(env) = isolated() else { return };`.
    pub fn isolated() -> Option<Env> {
        let thread = std::thread::current();
        let name = thread.name().expect("test thread must have its test name");
        if std::env::var("BRUMM_TEST_CASE").ok().as_deref() == Some(name) {
            return Some(Env {
                dir: std::env::var_os("BRUMM_TEST_HOME")
                    .expect("isolated test root")
                    .into(),
            });
        }
        let dir = std::env::temp_dir().join(format!(
            "brumm-test-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("run")).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let scope = Env { dir };
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("BRUMM_TEST_CASE", name)
            .env("BRUMM_TEST_HOME", &scope.dir)
            // No systemctl, hyprctl or omarchy: nothing outside the test home.
            .env("PATH", scope.dir.join("bin"))
            .env("XDG_CONFIG_HOME", scope.dir.join("config"))
            .env("XDG_CACHE_HOME", scope.dir.join("cache"))
            .env("XDG_RUNTIME_DIR", scope.dir.join("run"))
            .env("XDG_DATA_HOME", scope.dir.join("data"))
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .output()
            .expect("start isolated test process");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains("test result: ok. 1 passed;"),
            "isolated test {name} failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        None
    }

    pub fn rand_suffix() -> String {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).unwrap();
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn config_is_private_and_round_trips() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        assert_eq!(Config::load().unwrap(), Config::default());
        let c = Config {
            user_token: "secret".into(),
            ..Default::default()
        };
        c.save().unwrap();
        assert_eq!(Config::load().unwrap(), c);
        assert_eq!(fs::metadata(config_path()).unwrap().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(dir()).unwrap().mode() & 0o777, 0o700);
        let text = fs::read_to_string(config_path()).unwrap();
        assert!(
            !text.contains("developer_token"),
            "empty fields stay out: {text}"
        );
    }

    #[test]
    fn options_merge_types_and_legacy() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        let o = Options {
            cover: "smooth".into(),
            no_autoplay: true,
            ..Default::default()
        };
        o.save().unwrap();
        let mut set = Map::new();
        set.insert("start".into(), "gui".into());
        set.insert("no_autoplay".into(), false.into());
        set.insert("bogus".into(), 1.into());
        let n = Options::load().with(&set).unwrap();
        assert_eq!(n.start, "gui");
        assert!(!n.no_autoplay);
        assert_eq!(n.cover, "smooth");
        set.insert("viz_fps".into(), "fast".into());
        assert!(
            Options::load().with(&set).is_err(),
            "took a word for a number"
        );
        fs::write(options_path(), r#"{"no_cards":true}"#).unwrap();
        assert!(Options::load().reduce_motion);
    }

    #[test]
    fn expiry_reads_the_jwt() {
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1900000000}"#);
        assert_eq!(
            token_expiry(&format!("e30.{payload}.sig")),
            Some(1_900_000_000)
        );
        assert_eq!(token_expiry("nope"), None);
    }
}
