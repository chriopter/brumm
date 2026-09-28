// Package chrome provides the Google Chrome build brumm plays through.
//
// Omarchy's Chromium has no Widevine CDM, and Apple Music full tracks are
// Widevine-protected, so brumm keeps a private copy of Google Chrome (which
// bundles the CDM) in its cache directory.
package chrome

import (
	"bytes"
	"crypto/sha256"
	_ "embed"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/chriopter/brumm/internal/config"
)

// repo is Google's apt repository. Its InRelease file is signed with
// Google's Linux package key and lists the checksum of the package index,
// which lists the checksum of the .deb: a chain from the pinned key to the
// program brumm runs.
const repo = "https://dl.google.com/linux/chrome/deb/"

// googleKey is Google's Linux package signing key (from
// dl.google.com/linux/linux_signing_key.pub), primary key fingerprint
// EB4C 1BFD 4F04 2F6D DDCC EC91 7721 F63B D38B 4796.
//
//go:embed google-linux.gpg
var googleKey []byte

func installDir() string { return filepath.Join(config.CacheDir(), "chrome") }

func chromeBinary() string {
	return filepath.Join(installDir(), "opt", "google", "chrome", "chrome")
}

// Path is the executable to launch: a hard link to chrome named
// "brumm-helper", so the process tree reads as brumm in ps and btop. Chrome
// children re-exec /proc/self/exe and inherit the name.
func Path() string {
	return filepath.Join(filepath.Dir(chromeBinary()), "brumm-helper")
}

// WidevineDir is the CDM bundled with this Chrome, passed explicitly so no
// component-updater round trip is needed.
func WidevineDir() string {
	return filepath.Join(filepath.Dir(chromeBinary()), "WidevineCdm")
}

// Ensure installs Chrome into the cache when it is missing.
func Ensure(progress func(string)) error {
	if _, err := os.Stat(chromeBinary()); err != nil {
		progress("downloading Google Chrome (Widevine)…")
		if err := install(); err != nil {
			return fmt.Errorf("install chrome: %w", err)
		}
	}
	if _, err := os.Stat(Path()); err != nil {
		if err := os.Link(chromeBinary(), Path()); err != nil {
			return fmt.Errorf("link chrome helper: %w", err)
		}
	}
	return nil
}

// Bounds for the Chrome download: a stalled or runaway response must not
// hang startup or fill memory.
const (
	downloadTimeout = 15 * time.Minute
	maxDeb          = 400 << 20
)

func install() error {
	if err := os.MkdirAll(config.CacheDir(), 0o700); err != nil {
		return err
	}
	deb, err := download(&http.Client{Timeout: downloadTimeout})
	if err != nil {
		return err
	}
	name, data, err := arMember(deb, "data.tar")
	if err != nil {
		return err
	}

	// Extract next to the final location so the rename below stays on one
	// filesystem.
	tmpDir, err := os.MkdirTemp(config.CacheDir(), "chrome-install-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)
	archive := filepath.Join(tmpDir, name)
	if err := os.WriteFile(archive, data, 0o600); err != nil {
		return err
	}
	root := filepath.Join(tmpDir, "root")
	if err := os.Mkdir(root, 0o755); err != nil {
		return err
	}
	// tar recognises the xz/zstd/gzip compression by itself.
	if out, err := exec.Command("tar", "-xf", archive, "-C", root).CombinedOutput(); err != nil {
		return fmt.Errorf("extract: %v: %s", err, out)
	}
	_ = os.RemoveAll(installDir())
	return os.Rename(root, installDir())
}

// download fetches the current google-chrome-stable package and checks it
// against the signed repository metadata before returning it.
func download(client *http.Client) ([]byte, error) {
	dir, err := os.MkdirTemp(config.CacheDir(), "chrome-verify-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(dir)

	inRelease, err := get(client, repo+"dists/stable/InRelease", 1<<20)
	if err != nil {
		return nil, err
	}
	keyring, signed := filepath.Join(dir, "google.gpg"), filepath.Join(dir, "InRelease")
	if err := os.WriteFile(keyring, googleKey, 0o600); err != nil {
		return nil, err
	}
	if err := os.WriteFile(signed, inRelease, 0o600); err != nil {
		return nil, err
	}
	// gpgv exits non-zero unless the signature is good, and writes out only
	// the signed text, so nothing unsigned is read below.
	var stderr bytes.Buffer
	cmd := exec.Command("gpgv", "--keyring", keyring, "--output", "-", signed)
	cmd.Stderr = &stderr
	release, err := cmd.Output()
	if err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return nil, errors.New("gpgv is missing (install gnupg) — it checks Google's signature on Chrome")
		}
		return nil, fmt.Errorf("Chrome repository signature did not verify: %v %s", err, bytes.TrimSpace(stderr.Bytes()))
	}

	index := "main/binary-amd64/Packages"
	want := listedHash(release, index)
	if want == "" {
		return nil, errors.New("Chrome repository: no checksum for the package index")
	}
	packages, err := get(client, repo+"dists/stable/"+index, 8<<20)
	if err != nil {
		return nil, err
	}
	if !hashIs(packages, want) {
		return nil, errors.New("Chrome repository: package index does not match its signed checksum")
	}
	file, sum := stanza(packages, "google-chrome-stable")
	if !strings.HasPrefix(file, "pool/") || strings.Contains(file, "..") || sum == "" {
		return nil, errors.New("Chrome repository: google-chrome-stable not listed")
	}
	deb, err := get(client, repo+file, maxDeb)
	if err != nil {
		return nil, err
	}
	if !hashIs(deb, sum) {
		return nil, errors.New("Chrome package does not match its signed checksum")
	}
	return deb, nil
}

func get(client *http.Client, url string, limit int) ([]byte, error) {
	resp, err := client.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("download %s: %s", url, resp.Status)
	}
	b, err := io.ReadAll(io.LimitReader(resp.Body, int64(limit)+1))
	if err != nil {
		return nil, err
	}
	if len(b) > limit {
		return nil, fmt.Errorf("download %s: larger than expected", url)
	}
	return b, nil
}

func hashIs(b []byte, want string) bool {
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:]) == strings.ToLower(want)
}

// listedHash finds path's SHA256 in a Release file.
func listedHash(release []byte, path string) string {
	in := false
	for _, l := range strings.Split(string(release), "\n") {
		if !strings.HasPrefix(l, " ") {
			in = strings.TrimSpace(l) == "SHA256:"
			continue
		}
		if f := strings.Fields(l); in && len(f) == 3 && f[2] == path {
			return f[0]
		}
	}
	return ""
}

// stanza returns the Filename and SHA256 of a package in a Packages index.
func stanza(packages []byte, name string) (file, sum string) {
	for _, st := range strings.Split(string(packages), "\n\n") {
		fields := map[string]string{}
		for _, l := range strings.Split(st, "\n") {
			if k, v, ok := strings.Cut(l, ": "); ok {
				fields[k] = strings.TrimSpace(v)
			}
		}
		if fields["Package"] == name {
			return fields["Filename"], fields["SHA256"]
		}
	}
	return "", ""
}

// arMember returns the first member of a Debian ar archive whose name starts
// with prefix. Parsing it here avoids depending on dpkg.
func arMember(ar []byte, prefix string) (string, []byte, error) {
	const magic = "!<arch>\n"
	if !bytes.HasPrefix(ar, []byte(magic)) {
		return "", nil, errors.New("not a .deb archive")
	}
	for off := len(magic); off+60 <= len(ar); {
		hdr := ar[off : off+60]
		name := strings.TrimRight(strings.TrimSpace(string(hdr[0:16])), "/")
		size, err := strconv.Atoi(strings.TrimSpace(string(hdr[48:58])))
		if err != nil || off+60+size > len(ar) {
			return "", nil, errors.New("corrupt .deb archive")
		}
		body := ar[off+60 : off+60+size]
		if strings.HasPrefix(name, prefix) {
			return name, body, nil
		}
		off += 60 + size + size%2
	}
	return "", nil, fmt.Errorf("%s not found in .deb", prefix)
}

// Args are the launch flags for audio-only DRM playback.
func Args() []string {
	return []string{
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
		"--disable-features=HardwareMediaKeyHandling,MediaSessionService,CertificateTransparencyComponentUpdater",
		"--disable-component-update",
		// Memory: no GPU process, no /dev/shm pressure, capped V8 heap.
		"--disable-gpu",
		"--disable-dev-shm-usage",
		"--js-flags=--max-old-space-size=256",
		"--disable-background-networking",
		"--widevine-path=" + WidevineDir(),
		"--headless=new",
	}
}
