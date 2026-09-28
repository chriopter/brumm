// Package chrome provides the Google Chrome build brumm plays through.
//
// Omarchy's Chromium has no Widevine CDM, and Apple Music full tracks are
// Widevine-protected, so brumm keeps a private copy of Google Chrome (which
// bundles the CDM) in its cache directory.
package chrome

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/chriopter/brumm/internal/config"
)

const debURL = "https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb"

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

func install() error {
	resp, err := http.Get(debURL)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download: %s", resp.Status)
	}
	deb, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}
	name, data, err := arMember(deb, "data.tar")
	if err != nil {
		return err
	}

	if err := os.MkdirAll(config.CacheDir(), 0o755); err != nil {
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
		// The setuid sandbox is unavailable to a binary outside a system path.
		"--no-sandbox",
		"--disable-setuid-sandbox",
		"--no-zygote",
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
