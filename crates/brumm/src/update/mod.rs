//! Installs brumm into the user's home and keeps it current from GitHub
//! releases — no package manager involved.
//!
//! Layout:
//!
//! ```text
//! ~/.local/bin/brumm                          the program
//! ~/.local/bin/brumm-gui                      its window, where the release has it
//! ~/.local/share/brumm/                       the release's Omarchy files
//! ~/.config/systemd/user/brumm.service        the daemon's unit
//! ~/.local/share/applications/brumm.desktop   the launcher entry
//! ~/.config/omarchy/plugins/chriopter.brumm → ~/.local/share/brumm/plugin
//! ```

mod keys;
mod plugin;

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub use keys::{can_music_keys, set_music_keys};
pub use plugin::{has_agent, has_omarchy, plugin_enabled, plugin_linked, set_plugin, which};

use crate::config::{self, home};
use crate::error::{Result, err};

/// Verifies release signatures: the workflow signs SHA256SUMS with the
/// matching private key (repository secret BRUMM_SIGNING_KEY), and an
/// update installs nothing that is not signed by it.
const PUBLIC_KEY: &str = "b75tAkLgE9PbLq7BKUf3akF3BDBBcamcqS/fCKxwao4=";

const REPO: &str = "chriopter/brumm";
const ASSET: &str = "brumm-linux-amd64.tar.gz";
const SUMS: &str = "SHA256SUMS";
pub(crate) const PLUGIN_ID: &str = "chriopter.brumm";

/// Bounds connecting and waiting for an answer, not the whole body, so a
/// slow line still finishes a download.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .user_agent(format!("brumm/{}", config::VERSION))
        .build()
        .unwrap_or_default()
}

// Paths of an installation.
pub fn bin_path() -> PathBuf {
    home().join(".local/bin/brumm")
}
fn gui_path() -> PathBuf {
    home().join(".local/bin/brumm-gui")
}
fn share_dir() -> PathBuf {
    home().join(".local/share/brumm")
}
fn unit_path() -> PathBuf {
    home().join(".config/systemd/user/brumm.service")
}
fn plugin_link() -> PathBuf {
    home().join(".config/omarchy/plugins").join(PLUGIN_ID)
}
fn desktop_path() -> PathBuf {
    home().join(".local/share/applications/brumm.desktop")
}

/// Whether this binary is the installed one, the only kind that updates
/// itself (a dev build or a copy elsewhere never does).
pub fn managed(version: &str) -> bool {
    if version == "dev" || !version.starts_with('v') {
        return false;
    }
    let Ok(me) = crate::launch::executable() else {
        return false;
    };
    let me = fs::canonicalize(&me).unwrap_or(me);
    let bin = fs::canonicalize(bin_path()).unwrap_or_else(|_| bin_path());
    me == bin
}

/// When set, replaces GitHub for tests: BRUMM_UPDATE_BASE/latest answers
/// the newest tag, BRUMM_UPDATE_BASE/<tag>/<file> the release files.
/// Signatures are checked all the same.
fn test_base() -> String {
    std::env::var("BRUMM_UPDATE_BASE").unwrap_or_default()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Release {
    tag_name: String,
    body: String,
    draft: bool,
    prerelease: bool,
}

async fn get_json<T: for<'de> Deserialize<'de>>(url: &str, limit: usize) -> Result<T> {
    let b = fetch_limited(url, limit, "github").await?;
    Ok(serde_json::from_slice(&b)?)
}

/// The newest release's tag.
pub async fn latest() -> Result<String> {
    Ok(latest_notes().await?.0)
}

/// The newest release's tag and what it says is new: the points of its
/// notes, as plain lines.
pub async fn latest_notes() -> Result<(String, Vec<String>)> {
    let base = test_base();
    if !base.is_empty() {
        let tag = fetch(&format!("{base}/latest")).await?;
        return Ok((String::from_utf8_lossy(&tag).trim().to_string(), Vec::new()));
    }
    let rel: Release = get_json(
        &format!("https://api.github.com/repos/{REPO}/releases/latest"),
        1 << 20,
    )
    .await?;
    Ok((rel.tag_name, points(&rel.body)))
}

/// The newest release's tag and what is new since current: the points of
/// every release in between, not only the newest, for someone who skipped
/// some. They are merged by section (New, Changed, Faster, Fixed), the
/// newest release's points first in each.
pub async fn notes_since(current: &str) -> Result<(String, Vec<String>)> {
    if !test_base().is_empty() {
        return latest_notes().await;
    }
    let rels: Vec<Release> = get_json(
        &format!("https://api.github.com/repos/{REPO}/releases?per_page=50"),
        4 << 20,
    )
    .await?;
    let mut latest = String::new();
    for r in &rels {
        if !r.draft && !r.prerelease && (latest.is_empty() || newer(&r.tag_name, &latest)) {
            latest = r.tag_name.clone();
        }
    }
    if latest.is_empty() {
        return Err(err("github: no release"));
    }
    let bodies: Vec<(String, String)> = rels
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease && newer(&r.tag_name, current))
        .map(|r| (r.tag_name, r.body))
        .collect();
    Ok((latest, merge(bodies)))
}

/// The sections of merged notes, in order; any other heading follows.
const SECTIONS: [&str; 4] = ["New", "Changed", "Faster", "Fixed"];

/// Puts the points of several releases together under their sections,
/// the newest release's points first in each.
fn merge(mut rels: Vec<(String, String)>) -> Vec<String> {
    rels.sort_by(|a, b| {
        if newer(&a.0, &b.0) {
            std::cmp::Ordering::Less
        } else if newer(&b.0, &a.0) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let mut order: Vec<String> = SECTIONS.iter().map(|s| s.to_string()).collect();
    let mut by: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (_, body) in &rels {
        let mut head = String::new();
        for p in points(body) {
            if let Some(h) = p.strip_prefix("# ") {
                head = h.to_string();
                if !order.contains(&head) {
                    order.push(head.clone());
                }
                continue;
            }
            by.entry(head.clone()).or_default().push(p);
        }
    }
    let mut out = Vec::new();
    for h in &order {
        if let Some(ps) = by.get(h).filter(|p| !p.is_empty()) {
            out.push(format!("# {h}"));
            out.extend(ps.iter().cloned());
        }
    }
    // Older notes have no sections: their points close the list, under a
    // heading of their own once there are sections above.
    if let Some(rest) = by.get("").filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            out.push("# Earlier".into());
        }
        out.extend(rest.iter().cloned());
    }
    out
}

/// Drops HTML tags (`</?x…>`) from s.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        let after = &rest[i + 1..];
        let tag = after.strip_prefix('/').unwrap_or(after);
        let is_tag = tag.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        match (is_tag, rest[i..].find('>')) {
            (true, Some(j)) => {
                out.push_str(&rest[..i]);
                rest = &rest[i + j + 1..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Turns Markdown links [text](url) into their text.
fn unlink(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        let tail = &rest[i + 1..];
        let link = tail.find(']').and_then(|j| {
            let after = &tail[j + 1..];
            let close = after.strip_prefix('(')?.find(')')?;
            Some((j, j + 1 + 1 + close + 1))
        });
        match link {
            Some((text_end, total)) => {
                out.push_str(&rest[..i]);
                out.push_str(&tail[..text_end]);
                rest = &tail[total..];
            }
            None => {
                out.push_str(&rest[..=i]);
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The list items of a release's notes without their markup: what a
/// player can show under "is available". The headings of their sections
/// come along, marked "# ".
pub fn points(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in body.split('\n') {
        let l = l.trim();
        if let Some(h) = l.strip_prefix("## ") {
            out.push(format!("# {}", h.trim()));
            continue;
        }
        let Some(l) = l.strip_prefix("- ") else {
            continue;
        };
        let l = unlink(&strip_tags(l))
            .replace("**", "")
            .replace(['`', '*'], "");
        let l = l.trim();
        if !l.is_empty() {
            out.push(l.to_string());
        }
    }
    out
}

/// Whether tag a is a later version than b (vMAJOR.MINOR.PATCH, optionally
/// -PRERELEASE, which ranks below its release).
pub fn newer(a: &str, b: &str) -> bool {
    let (na, pa) = parse(a);
    let (nb, pb) = parse(b);
    if na != nb {
        return na > nb;
    }
    // Same numbers: a release beats its pre-release, nothing else is newer.
    pa.is_empty() && !pb.is_empty()
}

fn parse(tag: &str) -> ([i64; 3], String) {
    let v = tag.strip_prefix('v').unwrap_or(tag);
    let v = v.split('+').next().unwrap_or("");
    let (v, pre) = v.split_once('-').unwrap_or((v, ""));
    let mut nums = [0i64; 3];
    for (i, part) in v.splitn(3, '.').enumerate() {
        nums[i] = part.parse().unwrap_or(0);
    }
    (nums, pre.to_string())
}

/// Installs the newest release when it is newer than current; says the
/// tag and whether it installed anything.
pub async fn update(current: &str) -> Result<(String, bool)> {
    let tag = latest().await?;
    if !newer(&tag, current) {
        return Ok((tag, false));
    }
    let dir = download(&tag).await?;
    let r = install(&dir.join("brumm"));
    let _ = fs::remove_dir_all(&dir);
    r.map(|_| (tag, true))
}

/// Puts the window of release tag back when it is missing. A brumm from
/// before the window (up to v0.6) updates with its own installer, which
/// knows only the program, so the release it installs comes without
/// brumm-gui. Says whether it put the window in place.
pub async fn restore_window(tag: &str) -> Result<bool> {
    if gui_path().exists() {
        return Ok(false);
    }
    let dir = download(tag).await?;
    let src = dir.join("brumm/brumm-gui");
    let r = (|| {
        if !src.exists() {
            return Ok(false); // this release has no window
        }
        let _lock = lock()?;
        replace(&src, &gui_path(), 0o755).map_err(|e| err(format!("install window: {e}")))?;
        Ok(true)
    })();
    let _ = fs::remove_dir_all(&dir);
    r
}

/// Fetches a release tarball, checks it against the release's SHA256SUMS
/// and unpacks it into a temporary directory.
async fn download(tag: &str) -> Result<PathBuf> {
    let base = match test_base() {
        b if b.is_empty() => format!("https://github.com/{REPO}/releases/download/{tag}/"),
        b => format!("{b}/{tag}/"),
    };
    let want = checksum(&format!("{base}{SUMS}"), tag).await?;
    let data = fetch_limited(&format!("{base}{ASSET}"), 200 << 20, ASSET).await?;
    let got: String = Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != want {
        return Err(err("download does not match its checksum"));
    }
    let dir = crate::engine::private_temp_dir(&std::env::temp_dir(), "brumm-update-")?;
    if let Err(e) = untar(&data, &dir) {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(dir)
}

/// Fetches SHA256SUMS and its signature, verifies the signature against
/// PUBLIC_KEY and returns the tarball's checksum. The signed list also
/// names its version, so an older signed release cannot be passed off as
/// a newer one.
async fn checksum(url: &str, tag: &str) -> Result<String> {
    let list = fetch(url).await?;
    let sig = fetch(&format!("{url}.sig")).await?;
    verify(&list, &sig)?;
    sums_for(&list, tag)
}

/// The tarball's checksum from a verified list signed for tag.
fn sums_for(list: &[u8], tag: &str) -> Result<String> {
    let (mut sum, mut version) = (String::new(), String::new());
    for line in String::from_utf8_lossy(list).lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() == 2 && f[0] == "version" {
            version = f[1].to_string();
        } else if f.len() == 2 && f[1].trim_start_matches('*') == ASSET {
            sum = f[0].to_string();
        }
    }
    if version != tag {
        return Err(err(format!(
            "release {tag} is signed as {version:?}; not installing"
        )));
    }
    if sum.is_empty() {
        return Err(err(format!("{SUMS} lists no checksum for {ASSET}")));
    }
    Ok(sum)
}

/// Checks a base64 Ed25519 signature of data against PUBLIC_KEY.
fn verify(data: &[u8], sig: &[u8]) -> Result<()> {
    verify_with(PUBLIC_KEY, data, sig)
}

fn verify_with(public_key: &str, data: &[u8], sig: &[u8]) -> Result<()> {
    let invalid = || err("release signature is invalid; not installing");
    let b64 = base64::engine::general_purpose::STANDARD;
    let pub_bytes: [u8; 32] = b64
        .decode(public_key)
        .ok()
        .and_then(|p| p.try_into().ok())
        .ok_or_else(invalid)?;
    let raw: [u8; 64] = b64
        .decode(String::from_utf8_lossy(sig).trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or_else(invalid)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).map_err(|_| invalid())?;
    key.verify_strict(data, &ed25519_dalek::Signature::from_bytes(&raw))
        .map_err(|_| invalid())
}

async fn fetch(url: &str) -> Result<Vec<u8>> {
    let name = url.rsplit('/').next().unwrap_or(url).to_string();
    fetch_limited(url, 1 << 20, &name).await
}

/// Fetches url, at most limit bytes (anything past it is cut off, as Go's
/// LimitReader did).
async fn fetch_limited(url: &str, limit: usize, name: &str) -> Result<Vec<u8>> {
    let mut resp = client().get(url).send().await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(err(format!("download {name}: {}", resp.status())));
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        let room = limit.saturating_sub(out.len());
        out.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// Unpacks a .tar.gz into dir: directories and plain files only, none
/// outside dir.
fn untar(data: &[u8], dir: &Path) -> Result<()> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(data));
    for entry in ar.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.into_owned();
        if name.is_absolute()
            || name
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(err(format!("unsafe path in archive: {}", name.display())));
        }
        let target = dir.join(&name);
        match entry.header().entry_type() {
            tar::EntryType::Directory => fs::create_dir_all(&target)?,
            tar::EntryType::Regular => {
                if let Some(p) = target.parent() {
                    fs::create_dir_all(p)?;
                }
                let mode = entry.header().mode().unwrap_or(0o644) & 0o755;
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                fs::write(&target, &buf)?;
                fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Puts an unpacked release (or a build of the source, for development)
/// in place: the program, the Omarchy files, the unit, the launcher entry
/// and the bar widget. from holds `brumm` and `omarchy/`. Everything is
/// staged and checked first; the program is swapped in last, and the
/// Omarchy files roll back if that fails. Installs never overlap.
pub fn install(from: &Path) -> Result<()> {
    for f in [
        "brumm",
        "omarchy/brumm.service",
        "omarchy/brumm.desktop",
        "omarchy/plugin/manifest.json",
    ] {
        if let Err(e) = fs::metadata(from.join(f)) {
            return Err(err(format!(
                "not a brumm release: {}: {e}",
                from.join(f).display()
            )));
        }
    }
    let _lock = lock()?;
    let share_parent = share_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    fs::create_dir_all(&share_parent)?;
    // Stage: the Omarchy files and the program, next to where they go.
    let staged = crate::engine::private_temp_dir(&share_parent, ".brumm-share-")?;
    let mut cleanup: Vec<PathBuf> = Vec::new();
    let result = (|| {
        copy_tree(&from.join("omarchy"), &staged)?;
        let bin = stage(&from.join("brumm"), &bin_path(), 0o755)
            .map_err(|e| err(format!("stage program: {e}")))?;
        cleanup.push(bin.clone());
        // The unit and launcher entry too, so the commit below only renames.
        let unit = stage(&staged.join("brumm.service"), &unit_path(), 0o644)
            .map_err(|e| err(format!("stage service: {e}")))?;
        cleanup.push(unit.clone());
        let desktop = stage(&staged.join("brumm.desktop"), &desktop_path(), 0o644)
            .map_err(|e| err(format!("stage launcher: {e}")))?;
        cleanup.push(desktop.clone());
        // The window, where the release has it: older ones do not.
        let mut window = None;
        if from.join("brumm-gui").exists() {
            let w = stage(&from.join("brumm-gui"), &gui_path(), 0o755)
                .map_err(|e| err(format!("stage window: {e}")))?;
            cleanup.push(w.clone());
            window = Some(w);
        }

        // Commit: Omarchy files, then the program; undo the files if needed.
        let mut old = staged.as_os_str().to_owned();
        old.push(".old");
        let old = PathBuf::from(old);
        let had_old = fs::rename(share_dir(), &old).is_ok();
        if let Err(e) = fs::rename(&staged, share_dir()) {
            if had_old {
                let _ = fs::rename(&old, share_dir());
            }
            return Err(e.into());
        }
        if let Err(e) = fs::rename(&bin, bin_path()) {
            let _ = fs::remove_dir_all(share_dir());
            if had_old {
                let _ = fs::rename(&old, share_dir());
            }
            return Err(err(format!("install program: {e}")));
        }
        let _ = fs::remove_dir_all(&old);
        if let Some(w) = window {
            fs::rename(&w, gui_path()).map_err(|e| err(format!("install window: {e}")))?;
        }
        // Renames in the same directories; the program is already in place.
        fs::rename(&unit, unit_path())?;
        fs::rename(&desktop, desktop_path())?;
        Ok(())
    })();
    for p in cleanup {
        let _ = fs::remove_file(p);
    }
    let _ = fs::remove_dir_all(&staged);
    result?;
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    link_plugin()
}

/// Serializes installs across processes: the daemon, `brumm update` and
/// dev/setup may otherwise run at once. Held until dropped.
fn lock() -> Result<fs::File> {
    let path = share_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
        .join(".brumm-install.lock");
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)?;
    let _ = f.set_permissions(fs::Permissions::from_mode(0o600));
    f.lock()?;
    Ok(f)
}

/// Points the Omarchy bar widget at the installed files, so it always
/// matches the program, and enables it on first install.
fn link_plugin() -> Result<()> {
    let target = share_dir().join("plugin");
    let link = plugin_link();
    if fs::read_link(&link).is_ok_and(|cur| cur == target) {
        plugin::rescan(); // the files behind the link changed
        return Ok(());
    }
    let mut fresh = true;
    if let Ok(meta) = fs::symlink_metadata(&link) {
        fresh = false;
        if meta.is_dir() {
            fs::remove_dir_all(&link)?; // an older copied install
        } else {
            fs::remove_file(&link)?;
        }
    }
    if let Some(p) = link.parent() {
        fs::create_dir_all(p)?;
    }
    std::os::unix::fs::symlink(&target, &link)?;
    plugin::rescan();
    if fresh && has_omarchy() {
        let _ = set_plugin(true);
    }
    Ok(())
}

/// Copies src next to dst and renames it over dst.
fn replace(src: &Path, dst: &Path, mode: u32) -> Result<()> {
    let tmp = stage(src, dst, mode)?;
    if let Err(e) = fs::rename(&tmp, dst) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Copies src into a uniquely named file in dst's directory, ready to be
/// renamed over dst.
fn stage(src: &Path, dst: &Path, mode: u32) -> Result<PathBuf> {
    let dir = dst.parent().map(Path::to_path_buf).unwrap_or_default();
    fs::create_dir_all(&dir)?;
    let mut input = fs::File::open(src)?;
    for _ in 0..10 {
        let tmp = dir.join(format!(".brumm-{}", crate::engine::random_hex(6)));
        let mut out = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        };
        let r = std::io::copy(&mut input, &mut out)
            .and_then(|_| out.set_permissions(fs::Permissions::from_mode(mode)))
            .and_then(|_| out.sync_all());
        if let Err(e) = r {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        return Ok(tmp);
    }
    Err(err("no free temporary name"))
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    let meta = fs::metadata(src)?;
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for e in fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
        return Ok(());
    }
    replace(src, dst, meta.permissions().mode() & 0o755)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::testenv;

    #[test]
    fn version_order() {
        for (a, b, want) in [
            ("v0.2.0", "v0.1.9", true),
            ("v0.10.0", "v0.9.0", true),
            ("v0.2.0", "v0.2.0", false),
            ("v0.1.1", "v0.2.0", false),
            ("v1.0.0", "v0.99.99", true),
            ("v1.2.10-rc1", "v1.2.9", true),
            ("v1.2.1", "v1.2.3-rc1", false),
            ("v1.2.3", "v1.2.3-rc1", true),
            ("v1.2.3-rc2", "v1.2.3", false),
        ] {
            assert_eq!(newer(a, b), want, "newer({a}, {b})");
        }
    }

    #[test]
    fn notes_points_and_merge() {
        let body = "# brumm 1\n\n![x](y.webp)\n\n## New\n- 🪟 **A window** — `brumm --gui`, <kbd>g</kbd> switches\n- see [0.7](https://example.com)\n\n**Update:** press U.";
        assert_eq!(
            points(body),
            vec!["# New", "🪟 A window — brumm --gui, g switches", "see 0.7"]
        );
        let got = merge(vec![
            (
                "v0.8.0".into(),
                "# brumm 0.8\n\n## New\n- a\n\n## Fixed\n- b\n".into(),
            ),
            ("v0.9.1".into(), "## Fixed\n- c\n\n## Odd\n- d\n".into()),
            ("v0.9.0".into(), "## Faster\n- e\n\n## New\n- f\n".into()),
            ("v0.6.2".into(), "- g\n".into()),
        ]);
        assert_eq!(
            got.join("|"),
            "# New|f|a|# Faster|e|# Fixed|c|b|# Odd|d|# Earlier|g"
        );
    }

    #[test]
    fn signatures_and_signed_versions() {
        // A key of our own: the release key's private half is a secret.
        let signing = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let public =
            base64::engine::general_purpose::STANDARD.encode(signing.verifying_key().to_bytes());
        let list = format!("version v1.2.3\n{} *{ASSET}\n", "ab".repeat(32));
        use ed25519_dalek::Signer;
        let sig = base64::engine::general_purpose::STANDARD
            .encode(signing.sign(list.as_bytes()).to_bytes());
        verify_with(&public, list.as_bytes(), sig.as_bytes()).expect("genuine signature rejected");
        let mut tampered = list.clone().into_bytes();
        tampered[0] ^= 1;
        assert!(
            verify_with(&public, &tampered, sig.as_bytes()).is_err(),
            "tampered list accepted"
        );
        assert!(verify_with(&public, list.as_bytes(), b"bm90IGEgc2lnbmF0dXJl").is_err());
        // The release key rejects what our key signed.
        assert!(verify(list.as_bytes(), sig.as_bytes()).is_err());
        assert_eq!(
            sums_for(list.as_bytes(), "v1.2.3").unwrap(),
            "ab".repeat(32)
        );
        assert!(
            sums_for(list.as_bytes(), "v1.2.4").is_err(),
            "an older signed release passed as a newer one"
        );
    }

    #[test]
    fn untar_refuses_paths_outside() {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o755);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, "brumm/brumm", &b"x"[..]).unwrap();
        let ok = b.into_inner().unwrap();
        let gz = |raw: &[u8]| {
            use std::io::Write;
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(raw).unwrap();
            e.finish().unwrap()
        };
        let dir =
            std::env::temp_dir().join(format!("brumm-untar-{}", crate::engine::random_hex(6)));
        fs::create_dir_all(&dir).unwrap();
        untar(&gz(&ok), &dir).unwrap();
        assert_eq!(fs::read(dir.join("brumm/brumm")).unwrap(), b"x");
        assert_eq!(
            fs::metadata(dir.join("brumm/brumm"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        // An entry climbing out, written by hand: tar::Builder refuses to.
        let mut raw = ok.clone();
        raw[..12].copy_from_slice(b"../evil\0\0\0\0\0");
        let mut h = tar::Header::from_byte_slice(&raw[..512]).clone();
        h.set_cksum();
        raw[..512].copy_from_slice(h.as_bytes());
        assert!(untar(&gz(&raw), &dir).is_err());
        assert!(!dir.parent().unwrap().join("evil").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A release with the window installs it next to brumm; one without it
    /// still installs.
    #[test]
    fn install_with_and_without_the_window() {
        let Some(env) = testenv::isolated() else {
            return;
        };
        for with_gui in [true, false] {
            let from = env.dir.join(format!("release-{with_gui}"));
            let mut files = vec![
                "brumm",
                "omarchy/brumm.service",
                "omarchy/brumm.desktop",
                "omarchy/plugin/manifest.json",
            ];
            if with_gui {
                files.push("brumm-gui");
            }
            for f in files {
                let p = from.join(f);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, f).unwrap();
                fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let _ = fs::remove_file(gui_path());
            install(&from).unwrap();
            assert_eq!(fs::read(bin_path()).unwrap(), b"brumm");
            assert_eq!(fs::read(unit_path()).unwrap(), b"omarchy/brumm.service");
            assert!(
                fs::read_link(plugin_link())
                    .unwrap()
                    .ends_with("brumm/plugin")
            );
            match fs::metadata(gui_path()) {
                Ok(m) => assert!(
                    with_gui && m.permissions().mode() & 0o111 != 0,
                    "brumm-gui from nowhere"
                ),
                Err(_) => assert!(!with_gui, "brumm-gui not installed"),
            }
        }
        assert!(
            install(&env.dir.join("nothing")).is_err(),
            "installed what is no release"
        );
    }

    /// A window in place is left alone, with nothing fetched.
    #[test]
    fn restore_window_in_place() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        fs::create_dir_all(gui_path().parent().unwrap()).unwrap();
        fs::write(gui_path(), "gui").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(
            !rt.block_on(restore_window("v0.9.1")).unwrap(),
            "the window is there: nothing to restore"
        );
    }

    /// The published release verifies against the built-in key. Needs the
    /// network: BRUMM_NET_TEST=1.
    #[tokio::test]
    async fn the_published_release_verifies() {
        if std::env::var_os("BRUMM_NET_TEST").is_none() {
            return;
        }
        let tag = latest().await.unwrap();
        let base = format!("https://github.com/{REPO}/releases/download/{tag}/");
        checksum(&format!("{base}{SUMS}"), &tag)
            .await
            .expect("latest release rejected");
        let mut list = fetch(&format!("{base}{SUMS}")).await.unwrap();
        let sig = fetch(&format!("{base}{SUMS}.sig")).await.unwrap();
        verify(&list, &sig).expect("genuine release rejected");
        list[0] ^= 1;
        assert!(verify(&list, &sig).is_err(), "tampered checksums accepted");
    }
}
