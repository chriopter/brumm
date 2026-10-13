//! The Google Chrome build brumm plays through.
//!
//! Omarchy's Chromium has no Widevine CDM, and Apple Music full tracks are
//! Widevine-protected, so brumm keeps a private copy of Google Chrome
//! (which bundles the CDM) in its cache directory.

use std::path::PathBuf;
use std::time::Duration;

use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::config;
use crate::error::{Result, err};

/// Google's apt repository. Its InRelease file is signed with Google's
/// Linux package key and lists the checksum of the package index, which
/// lists the checksum of the .deb: a chain from the pinned key to the
/// program brumm runs.
const REPO: &str = "https://dl.google.com/linux/chrome/deb/";

/// Google's Linux package signing key (from
/// dl.google.com/linux/linux_signing_key.pub), primary key fingerprint
/// EB4C 1BFD 4F04 2F6D DDCC EC91 7721 F63B D38B 4796; kept as base64 text
/// of the binary keyring gpgv reads.
const GOOGLE_KEY_B64: &str = include_str!("google-linux.gpg.b64");

fn google_key() -> Vec<u8> {
    let text: String = GOOGLE_KEY_B64.split_whitespace().collect();
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap_or_default()
}

fn install_dir() -> PathBuf {
    config::cache_dir().join("chrome")
}

fn chrome_binary() -> PathBuf {
    install_dir().join("opt/google/chrome/chrome")
}

/// The executable to launch: a hard link to chrome named "brumm-helper",
/// so the process tree reads as brumm in ps and btop. Chrome children
/// re-exec /proc/self/exe and inherit the name.
pub fn path() -> PathBuf {
    chrome_binary().with_file_name("brumm-helper")
}

/// The CDM bundled with this Chrome, passed explicitly so no
/// component-updater round trip is needed.
fn widevine_dir() -> PathBuf {
    chrome_binary().with_file_name("WidevineCdm")
}

/// Installs Chrome into the cache when it is missing.
pub async fn ensure(progress: impl Fn(&str)) -> Result<()> {
    if !chrome_binary().exists() {
        progress("downloading Google Chrome (Widevine)…");
        install()
            .await
            .map_err(|e| err(format!("install chrome: {e}")))?;
    }
    if !path().exists() {
        std::fs::hard_link(chrome_binary(), path())
            .map_err(|e| err(format!("link chrome helper: {e}")))?;
    }
    Ok(())
}

/// Bounds for the Chrome download: a stalled or runaway response must not
/// hang startup or fill memory.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_DEB: usize = 400 << 20;

async fn install() -> Result<()> {
    std::fs::create_dir_all(config::cache_dir())?;
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()?;
    let deb = download(&client).await?;
    let (name, data) = ar_member(&deb, "data.tar")?;
    // Extract next to the final location so the rename stays on one
    // filesystem.
    let tmp = crate::engine::private_temp_dir(&config::cache_dir(), "chrome-install-")?;
    let result = async {
        let archive = tmp.join(&name);
        std::fs::write(&archive, data)?;
        let root = tmp.join("root");
        std::fs::create_dir(&root)?;
        // tar recognises the xz/zstd/gzip compression by itself.
        let out = tokio::process::Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&root)
            .output()
            .await?;
        if !out.status.success() {
            return Err(err(format!(
                "extract: {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let _ = std::fs::remove_dir_all(install_dir());
        std::fs::rename(&root, install_dir())?;
        Ok(())
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// The current google-chrome-stable package, checked against the signed
/// repository metadata before it is returned.
async fn download(client: &reqwest::Client) -> Result<Vec<u8>> {
    let in_release = get(client, &format!("{REPO}dists/stable/InRelease"), 1 << 20).await?;
    let release = verify_release(&config::cache_dir(), &in_release).await?;
    let index = "main/binary-amd64/Packages";
    let want = listed_hash(&release, index);
    if want.is_empty() {
        return Err(err("Chrome repository: no checksum for the package index"));
    }
    let packages = get(client, &format!("{REPO}dists/stable/{index}"), 8 << 20).await?;
    if !hash_is(&packages, &want) {
        return Err(err(
            "Chrome repository: package index does not match its signed checksum",
        ));
    }
    let (file, sum) = stanza(&String::from_utf8_lossy(&packages), "google-chrome-stable");
    if !file.starts_with("pool/") || file.contains("..") || sum.is_empty() {
        return Err(err("Chrome repository: google-chrome-stable not listed"));
    }
    let deb = get(client, &format!("{REPO}{file}"), MAX_DEB).await?;
    if !hash_is(&deb, &sum) {
        return Err(err("Chrome package does not match its signed checksum"));
    }
    Ok(deb)
}

/// Checks InRelease against Google's key with gpgv, which exits non-zero
/// unless the signature is good, and returns only the signed text, so
/// nothing unsigned is read after.
async fn verify_release(scratch: &std::path::Path, in_release: &[u8]) -> Result<String> {
    let dir = crate::engine::private_temp_dir(scratch, "chrome-verify-")?;
    let result = async {
        let (keyring, signed) = (dir.join("google.gpg"), dir.join("InRelease"));
        std::fs::write(&keyring, google_key())?;
        std::fs::write(&signed, in_release)?;
        let out = tokio::process::Command::new("gpgv")
            .arg("--keyring")
            .arg(&keyring)
            .arg("--output")
            .arg("-")
            .arg(&signed)
            .stdin(std::process::Stdio::null())
            .output()
            .await;
        let out = match out {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(err(
                    "gpgv is missing (install gnupg) — it checks Google's signature on Chrome",
                ));
            }
            r => r?,
        };
        if !out.status.success() {
            return Err(err(format!(
                "Chrome repository signature did not verify: {} {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    .await;
    let _ = std::fs::remove_dir_all(&dir);
    result
}

async fn get(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut resp = client.get(url).send().await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(err(format!("download {url}: {}", resp.status())));
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        out.extend_from_slice(&chunk);
        if out.len() > limit {
            return Err(err(format!("download {url}: larger than expected")));
        }
    }
    Ok(out)
}

fn hash_is(b: &[u8], want: &str) -> bool {
    let sum = Sha256::digest(b);
    sum.iter().map(|x| format!("{x:02x}")).collect::<String>() == want.to_lowercase()
}

/// path's SHA256 in a Release file.
fn listed_hash(release: &str, path: &str) -> String {
    let mut inside = false;
    for l in release.split('\n') {
        if !l.starts_with(' ') {
            inside = l.trim() == "SHA256:";
            continue;
        }
        let f: Vec<&str> = l.split_whitespace().collect();
        if inside && f.len() == 3 && f[2] == path {
            return f[0].to_string();
        }
    }
    String::new()
}

/// The Filename and SHA256 of a package in a Packages index.
fn stanza(packages: &str, name: &str) -> (String, String) {
    for st in packages.split("\n\n") {
        let mut fields = std::collections::HashMap::new();
        for l in st.split('\n') {
            if let Some((k, v)) = l.split_once(": ") {
                fields.insert(k, v.trim());
            }
        }
        if fields.get("Package") == Some(&name) {
            return (
                fields.get("Filename").unwrap_or(&"").to_string(),
                fields.get("SHA256").unwrap_or(&"").to_string(),
            );
        }
    }
    (String::new(), String::new())
}

/// The first member of a Debian ar archive whose name starts with prefix.
/// Parsing it here avoids depending on dpkg.
fn ar_member<'a>(ar: &'a [u8], prefix: &str) -> Result<(String, &'a [u8])> {
    const MAGIC: &[u8] = b"!<arch>\n";
    if !ar.starts_with(MAGIC) {
        return Err(err("not a .deb archive"));
    }
    let mut off = MAGIC.len();
    while off + 60 <= ar.len() {
        let hdr = &ar[off..off + 60];
        let name = String::from_utf8_lossy(&hdr[0..16])
            .trim()
            .trim_end_matches('/')
            .to_string();
        let size: usize = String::from_utf8_lossy(&hdr[48..58])
            .trim()
            .parse()
            .map_err(|_| err("corrupt .deb archive"))?;
        if off + 60 + size > ar.len() {
            return Err(err("corrupt .deb archive"));
        }
        let body = &ar[off + 60..off + 60 + size];
        if name.starts_with(prefix) {
            // The member's name, as a plain file name.
            if name.contains('/') || name.starts_with('.') {
                return Err(err("corrupt .deb archive"));
            }
            return Ok((name, body));
        }
        off += 60 + size + size % 2;
    }
    Err(err(format!("{prefix} not found in .deb")))
}

/// The launch flags for audio-only DRM playback.
pub fn args() -> Vec<String> {
    let mut a: Vec<String> = [
        // The setuid sandbox needs a root-owned helper, which a copy in the
        // user's cache cannot have; Chrome falls back to its user-namespace
        // sandbox, which Arch enables. The page stays sandboxed.
        "--disable-setuid-sandbox",
        // There is never a user gesture in a headless page.
        "--autoplay-policy=no-user-gesture-required",
        "--enable-features=MediaCapabilities,WidevineCdm",
        "--disable-blink-features=AutomationControlled",
        // Keep Chrome from registering its own MPRIS player and grabbing
        // media keys: brumm's daemon is the one player.
        "--disable-features=HardwareMediaKeyHandling,MediaSessionService,CertificateTransparencyComponentUpdater,Translate,MediaRouter,OptimizationHints,AutofillServerCommunication",
        "--disable-component-update",
        // Nothing a music page needs, and each one is a timer or a request.
        "--disable-sync",
        "--disable-extensions",
        "--disable-default-apps",
        "--disable-client-side-phishing-detection",
        "--disable-domain-reliability",
        "--disable-breakpad",
        "--metrics-recording-only",
        "--no-pings",
        // Memory: no GPU process, no /dev/shm pressure, capped V8 heap.
        "--disable-gpu",
        "--disable-dev-shm-usage",
        "--js-flags=--max-old-space-size=256",
        "--disable-background-networking",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(format!("--widevine-path={}", widevine_dir().display()));
    a.push("--headless=new".into());
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_key_is_googles() {
        // The keyring as shipped before, byte for byte.
        let sum = Sha256::digest(google_key());
        let hex: String = sum.iter().map(|x| format!("{x:02x}")).collect();
        assert_eq!(
            hex,
            "0631021cd0efb5ebe428b09b003e4dc45f9c8cd1c47befd5196906768eff96e0"
        );
    }

    #[test]
    fn release_and_index_parsing() {
        let release = "Origin: Google LLC\nMD5Sum:\n 0123 6207 main/binary-amd64/Packages\nSHA256:\n aaaa 1411 main/binary-amd64/Packages.gz\n bbbb 6207 main/binary-amd64/Packages\n";
        assert_eq!(listed_hash(release, "main/binary-amd64/Packages"), "bbbb");
        assert_eq!(listed_hash(release, "main/binary-arm64/Packages"), "");
        let pk = "Package: google-chrome-beta\nFilename: pool/beta.deb\nSHA256: 11\n\nPackage: google-chrome-stable\nVersion: 1\nFilename: pool/stable.deb\nSHA256: 22\n";
        assert_eq!(
            stanza(pk, "google-chrome-stable"),
            ("pool/stable.deb".into(), "22".into())
        );
        assert!(hash_is(
            b"",
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        ));
    }

    #[test]
    fn ar_members_are_bounded() {
        let mut ar = b"!<arch>\n".to_vec();
        let header = |name: &str, size: usize| {
            format!(
                "{name:<16}{:<12}{:<6}{:<6}{:<8}{size:<10}`\n",
                0, 0, 0, 100644
            )
        };
        ar.extend_from_slice(header("debian-binary", 4).as_bytes());
        ar.extend_from_slice(b"2.0\n");
        ar.extend_from_slice(header("data.tar.xz", 3).as_bytes());
        ar.extend_from_slice(b"abc\n");
        let (name, body) = ar_member(&ar, "data.tar").unwrap();
        assert_eq!((name.as_str(), body), ("data.tar.xz", &b"abc"[..]));
        assert!(ar_member(b"nope", "data.tar").is_err());
        let mut short = b"!<arch>\n".to_vec();
        short.extend_from_slice(header("data.tar.xz", 999).as_bytes());
        assert!(
            ar_member(&short, "data.tar").is_err(),
            "a size past the end was believed"
        );
    }

    /// The live repository: the signed InRelease verifies and one changed
    /// byte does not. BRUMM_NET=1 enables it.
    #[tokio::test]
    async fn signature_chain_against_google() {
        if std::env::var_os("BRUMM_NET").is_none() {
            return;
        }
        let client = reqwest::Client::new();
        let in_release = get(&client, &format!("{REPO}dists/stable/InRelease"), 1 << 20)
            .await
            .unwrap();
        let tmp = std::env::temp_dir();
        let rel = verify_release(&tmp, &in_release)
            .await
            .expect("genuine InRelease rejected");
        assert!(!listed_hash(&rel, "main/binary-amd64/Packages").is_empty());
        let text = String::from_utf8_lossy(&in_release).replacen(
            "main/binary-amd64/Packages\n",
            "main/binary-amd64/Packagez\n",
            1,
        );
        assert!(
            verify_release(&tmp, text.as_bytes()).await.is_err(),
            "tampered InRelease accepted"
        );
    }
}
