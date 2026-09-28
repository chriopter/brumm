// Package update installs brumm into the user's home and keeps it current
// from GitHub releases — no package manager involved.
//
// Layout:
//
//	~/.local/bin/brumm                          the program
//	~/.local/share/brumm/                       the release's Omarchy files
//	~/.config/systemd/user/brumm.service        the daemon's unit
//	~/.local/share/applications/brumm.desktop   the launcher entry
//	~/.config/omarchy/plugins/chriopter.brumm → ~/.local/share/brumm/plugin
package update

import (
	"archive/tar"
	"bufio"
	"compress/gzip"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
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
)

// publicKey verifies release signatures: the workflow signs SHA256SUMS
// with the matching private key (repository secret BRUMM_SIGNING_KEY), and
// an update installs nothing that is not signed by it.
const publicKey = "b75tAkLgE9PbLq7BKUf3akF3BDBBcamcqS/fCKxwao4="

const (
	repo     = "chriopter/brumm"
	asset    = "brumm-linux-amd64.tar.gz"
	sums     = "SHA256SUMS"
	pluginID = "chriopter.brumm"
)

var client = &http.Client{Timeout: 2 * time.Minute}

func home() string { h, _ := os.UserHomeDir(); return h }

// Paths of an installation.
func BinPath() string    { return filepath.Join(home(), ".local", "bin", "brumm") }
func shareDir() string   { return filepath.Join(home(), ".local", "share", "brumm") }
func unitPath() string   { return filepath.Join(home(), ".config", "systemd", "user", "brumm.service") }
func pluginLink() string { return filepath.Join(home(), ".config", "omarchy", "plugins", pluginID) }
func desktopPath() string {
	return filepath.Join(home(), ".local", "share", "applications", "brumm.desktop")
}

// Managed reports whether this binary is the installed one, the only kind
// that updates itself (a dev build or a copy elsewhere never does).
func Managed(version string) bool {
	self, err := os.Executable()
	if err != nil || version == "dev" || !strings.HasPrefix(version, "v") {
		return false
	}
	self, _ = filepath.EvalSymlinks(self)
	bin, _ := filepath.EvalSymlinks(BinPath())
	return self == bin
}

// Latest returns the newest release's tag.
func Latest() (string, error) {
	resp, err := client.Get("https://api.github.com/repos/" + repo + "/releases/latest")
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("github: %s", resp.Status)
	}
	var rel struct {
		Tag string `json:"tag_name"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return "", err
	}
	return rel.Tag, nil
}

// Newer reports whether tag a is a later version than b (vMAJOR.MINOR.PATCH).
func Newer(a, b string) bool {
	pa, pb := parse(a), parse(b)
	for i := range pa {
		if pa[i] != pb[i] {
			return pa[i] > pb[i]
		}
	}
	return false
}

func parse(tag string) [3]int {
	var v [3]int
	for i, part := range strings.SplitN(strings.TrimPrefix(tag, "v"), ".", 3) {
		n, _ := strconv.Atoi(strings.TrimFunc(part, func(r rune) bool { return r < '0' || r > '9' }))
		v[i] = n
	}
	return v
}

// Update installs tag when it is newer than current. It reports whether it
// installed anything.
func Update(current string) (string, bool, error) {
	tag, err := Latest()
	if err != nil {
		return "", false, err
	}
	if !Newer(tag, current) {
		return tag, false, nil
	}
	dir, err := download(tag)
	if err != nil {
		return tag, false, err
	}
	defer os.RemoveAll(dir)
	return tag, true, Install(filepath.Join(dir, "brumm"))
}

// download fetches a release tarball, checks it against the release's
// SHA256SUMS and unpacks it into a temporary directory.
func download(tag string) (string, error) {
	base := "https://github.com/" + repo + "/releases/download/" + tag + "/"
	want, err := checksum(base + sums)
	if err != nil {
		return "", err
	}
	resp, err := client.Get(base + asset)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("download %s: %s", asset, resp.Status)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 200<<20))
	if err != nil {
		return "", err
	}
	if got := sha256.Sum256(data); hex.EncodeToString(got[:]) != want {
		return "", errors.New("download does not match its checksum")
	}
	dir, err := os.MkdirTemp("", "brumm-update-")
	if err != nil {
		return "", err
	}
	if err := untar(data, dir); err != nil {
		os.RemoveAll(dir)
		return "", err
	}
	return dir, nil
}

// checksum fetches SHA256SUMS and its signature, verifies the signature
// against publicKey and returns the tarball's checksum from it.
func checksum(url string) (string, error) {
	list, err := fetch(url)
	if err != nil {
		return "", err
	}
	sig, err := fetch(url + ".sig")
	if err != nil {
		return "", err
	}
	pub, _ := base64.StdEncoding.DecodeString(publicKey)
	raw, err := base64.StdEncoding.DecodeString(strings.TrimSpace(string(sig)))
	if err != nil || len(pub) != ed25519.PublicKeySize || !ed25519.Verify(pub, list, raw) {
		return "", errors.New("release signature is invalid; not installing")
	}
	sc := bufio.NewScanner(strings.NewReader(string(list)))
	for sc.Scan() {
		if f := strings.Fields(sc.Text()); len(f) == 2 && strings.TrimPrefix(f[1], "*") == asset {
			return f[0], nil
		}
	}
	return "", fmt.Errorf("%s lists no checksum for %s", sums, asset)
}

func fetch(url string) ([]byte, error) {
	resp, err := client.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("download %s: %s", filepath.Base(url), resp.Status)
	}
	return io.ReadAll(io.LimitReader(resp.Body, 1<<20))
}

func untar(data []byte, dir string) error {
	gz, err := gzip.NewReader(strings.NewReader(string(data)))
	if err != nil {
		return err
	}
	tr := tar.NewReader(gz)
	for {
		h, err := tr.Next()
		if errors.Is(err, io.EOF) {
			return nil
		}
		if err != nil {
			return err
		}
		target := filepath.Join(dir, h.Name)
		if !strings.HasPrefix(target, filepath.Clean(dir)+string(os.PathSeparator)) {
			return fmt.Errorf("unsafe path in archive: %s", h.Name)
		}
		switch h.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(target, 0o755); err != nil {
				return err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
				return err
			}
			f, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, os.FileMode(h.Mode)&0o755)
			if err != nil {
				return err
			}
			_, err = io.Copy(f, tr)
			f.Close()
			if err != nil {
				return err
			}
		}
	}
}

// Install puts an unpacked release (or a source checkout, for development)
// in place: the program, the Omarchy files, the unit, the launcher entry
// and the bar widget. from holds `brumm` and `omarchy/`. Every file is
// replaced by rename, so a running brumm keeps working until it restarts.
func Install(from string) error {
	if err := replace(filepath.Join(from, "brumm"), BinPath(), 0o755); err != nil {
		return fmt.Errorf("install program: %w", err)
	}
	// The Omarchy files: swap the whole directory in.
	staged := shareDir() + ".new"
	os.RemoveAll(staged)
	if err := copyTree(filepath.Join(from, "omarchy"), staged); err != nil {
		return err
	}
	old := shareDir() + ".old"
	os.RemoveAll(old)
	_ = os.Rename(shareDir(), old)
	if err := os.Rename(staged, shareDir()); err != nil {
		return err
	}
	os.RemoveAll(old)

	if err := replace(filepath.Join(shareDir(), "brumm.service"), unitPath(), 0o644); err != nil {
		return err
	}
	if err := replace(filepath.Join(shareDir(), "brumm.desktop"), desktopPath(), 0o644); err != nil {
		return err
	}
	_ = exec.Command("systemctl", "--user", "daemon-reload").Run()
	return linkPlugin()
}

// linkPlugin points the Omarchy bar widget at the installed files, so it
// always matches the program, and enables it on first install.
func linkPlugin() error {
	target := filepath.Join(shareDir(), "plugin")
	link := pluginLink()
	if cur, err := os.Readlink(link); err == nil && cur == target {
		return nil
	}
	fresh := true
	if _, err := os.Lstat(link); err == nil {
		fresh = false
		if err := os.RemoveAll(link); err != nil { // an older copied install
			return err
		}
	}
	if err := os.MkdirAll(filepath.Dir(link), 0o755); err != nil {
		return err
	}
	if err := os.Symlink(target, link); err != nil {
		return err
	}
	if _, err := exec.LookPath("omarchy-shell"); err == nil {
		_ = exec.Command("omarchy-shell", "shell", "rescanPlugins").Run()
		if fresh {
			_ = exec.Command("omarchy", "plugin", "enable", pluginID).Run()
		}
	}
	return nil
}

// replace copies src next to dst and renames it over dst.
func replace(src, dst string, mode os.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	tmp := dst + ".new"
	out, err := os.OpenFile(tmp, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, mode)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	if err := out.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, dst)
}

func copyTree(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(src, path)
		target := filepath.Join(dst, rel)
		if info.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		return replace(path, target, info.Mode()&0o755)
	})
}
