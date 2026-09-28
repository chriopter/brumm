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
	"bytes"
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
	"syscall"
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

// client bounds connecting and waiting for an answer, not the whole body,
// so a slow line still finishes a download.
var client = &http.Client{Transport: &http.Transport{
	Proxy:                 http.ProxyFromEnvironment,
	TLSHandshakeTimeout:   15 * time.Second,
	ResponseHeaderTimeout: 30 * time.Second,
	IdleConnTimeout:       30 * time.Second,
}}

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

// testBase, when set, replaces GitHub for tests: BRUMM_UPDATE_BASE/latest
// answers the newest tag, BRUMM_UPDATE_BASE/<tag>/<file> the release files.
// Signatures are checked all the same.
func testBase() string { return os.Getenv("BRUMM_UPDATE_BASE") }

// Latest returns the newest release's tag.
func Latest() (string, error) {
	if b := testBase(); b != "" {
		tag, err := fetch(b + "/latest")
		return strings.TrimSpace(string(tag)), err
	}
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

// Newer reports whether tag a is a later version than b
// (vMAJOR.MINOR.PATCH, optionally -PRERELEASE, which ranks below its
// release).
func Newer(a, b string) bool {
	na, pa := parse(a)
	nb, pb := parse(b)
	if na != nb {
		for i := range na {
			if na[i] != nb[i] {
				return na[i] > nb[i]
			}
		}
	}
	// Same numbers: a release beats its pre-release, nothing else is newer.
	return pa == "" && pb != ""
}

func parse(tag string) (nums [3]int, pre string) {
	v := strings.TrimPrefix(tag, "v")
	if i := strings.IndexAny(v, "+"); i >= 0 {
		v = v[:i]
	}
	if i := strings.IndexByte(v, '-'); i >= 0 {
		v, pre = v[:i], v[i+1:]
	}
	for i, part := range strings.SplitN(v, ".", 3) {
		nums[i], _ = strconv.Atoi(part)
	}
	return nums, pre
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
	if b := testBase(); b != "" {
		base = b + "/" + tag + "/"
	}
	want, err := checksum(base+sums, tag)
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
// against publicKey and returns the tarball's checksum. The signed list
// also names its version, so an older signed release cannot be passed off
// as a newer one.
func checksum(url, tag string) (string, error) {
	list, err := fetch(url)
	if err != nil {
		return "", err
	}
	sig, err := fetch(url + ".sig")
	if err != nil {
		return "", err
	}
	if err := verify(list, sig); err != nil {
		return "", err
	}
	sum, version := "", ""
	sc := bufio.NewScanner(bytes.NewReader(list))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		switch {
		case len(f) == 2 && f[0] == "version":
			version = f[1]
		case len(f) == 2 && strings.TrimPrefix(f[1], "*") == asset:
			sum = f[0]
		}
	}
	if version != tag {
		return "", fmt.Errorf("release %s is signed as %q; not installing", tag, version)
	}
	if sum == "" {
		return "", fmt.Errorf("%s lists no checksum for %s", sums, asset)
	}
	return sum, nil
}

// verify checks a base64 Ed25519 signature of data against publicKey.
func verify(data, sig []byte) error {
	pub, _ := base64.StdEncoding.DecodeString(publicKey)
	raw, err := base64.StdEncoding.DecodeString(strings.TrimSpace(string(sig)))
	if err != nil || len(pub) != ed25519.PublicKeySize || !ed25519.Verify(pub, data, raw) {
		return errors.New("release signature is invalid; not installing")
	}
	return nil
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
	gz, err := gzip.NewReader(bytes.NewReader(data))
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
		if target != filepath.Clean(dir) && !strings.HasPrefix(target, filepath.Clean(dir)+string(os.PathSeparator)) {
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
// and the bar widget. from holds `brumm` and `omarchy/`. Everything is
// staged and checked first; the program is swapped in last, and the
// Omarchy files roll back if that fails. Installs never overlap.
func Install(from string) error {
	for _, f := range []string{"brumm", "omarchy/brumm.service", "omarchy/brumm.desktop", "omarchy/plugin/manifest.json"} {
		if _, err := os.Stat(filepath.Join(from, f)); err != nil {
			return fmt.Errorf("not a brumm release: %w", err)
		}
	}
	unlock, err := lock()
	if err != nil {
		return err
	}
	defer unlock()

	// Stage: the Omarchy files and the program, next to where they go.
	if err := os.MkdirAll(filepath.Dir(shareDir()), 0o755); err != nil {
		return err
	}
	staged, err := os.MkdirTemp(filepath.Dir(shareDir()), ".brumm-share-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(staged)
	if err := copyTree(filepath.Join(from, "omarchy"), staged); err != nil {
		return err
	}
	bin, err := stage(filepath.Join(from, "brumm"), BinPath(), 0o755)
	if err != nil {
		return fmt.Errorf("stage program: %w", err)
	}
	defer os.Remove(bin)
	// The unit and launcher entry too, so the commit below only renames.
	unit, err := stage(filepath.Join(staged, "brumm.service"), unitPath(), 0o644)
	if err != nil {
		return fmt.Errorf("stage service: %w", err)
	}
	defer os.Remove(unit)
	desktop, err := stage(filepath.Join(staged, "brumm.desktop"), desktopPath(), 0o644)
	if err != nil {
		return fmt.Errorf("stage launcher: %w", err)
	}
	defer os.Remove(desktop)

	// Commit: Omarchy files, then the program; undo the files if needed.
	old := staged + ".old"
	hadOld := os.Rename(shareDir(), old) == nil
	if err := os.Rename(staged, shareDir()); err != nil {
		if hadOld {
			_ = os.Rename(old, shareDir())
		}
		return err
	}
	if err := os.Rename(bin, BinPath()); err != nil {
		_ = os.RemoveAll(shareDir())
		if hadOld {
			_ = os.Rename(old, shareDir())
		}
		return fmt.Errorf("install program: %w", err)
	}
	os.RemoveAll(old)

	// Renames in the same directories; the program is already in place.
	if err := os.Rename(unit, unitPath()); err != nil {
		return err
	}
	if err := os.Rename(desktop, desktopPath()); err != nil {
		return err
	}
	_ = exec.Command("systemctl", "--user", "daemon-reload").Run()
	return linkPlugin()
}

// lock serializes installs across processes: the daemon, `brumm update`
// and bin/setup may otherwise run at once.
func lock() (func(), error) {
	path := filepath.Join(filepath.Dir(shareDir()), ".brumm-install.lock")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX); err != nil {
		f.Close()
		return nil, err
	}
	return func() { _ = syscall.Flock(int(f.Fd()), syscall.LOCK_UN); f.Close() }, nil
}

// linkPlugin points the Omarchy bar widget at the installed files, so it
// always matches the program, and enables it on first install.
func linkPlugin() error {
	target := filepath.Join(shareDir(), "plugin")
	link := pluginLink()
	if cur, err := os.Readlink(link); err == nil && cur == target {
		rescan() // the files behind the link changed
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
	rescan()
	if fresh {
		if _, err := exec.LookPath("omarchy"); err == nil {
			_ = exec.Command("omarchy", "plugin", "enable", pluginID).Run()
		}
	}
	return nil
}

// rescan makes the Omarchy shell load the widget's files again.
func rescan() {
	if _, err := exec.LookPath("omarchy-shell"); err == nil {
		_ = exec.Command("omarchy-shell", "shell", "rescanPlugins").Run()
	}
}

// replace copies src next to dst and renames it over dst.
func replace(src, dst string, mode os.FileMode) error {
	tmp, err := stage(src, dst, mode)
	if err != nil {
		return err
	}
	if err := os.Rename(tmp, dst); err != nil {
		os.Remove(tmp)
		return err
	}
	return nil
}

// stage copies src into a uniquely named file in dst's directory, ready to
// be renamed over dst.
func stage(src, dst string, mode os.FileMode) (string, error) {
	in, err := os.Open(src)
	if err != nil {
		return "", err
	}
	defer in.Close()
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return "", err
	}
	out, err := os.CreateTemp(filepath.Dir(dst), ".brumm-*")
	if err != nil {
		return "", err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		os.Remove(out.Name())
		return "", err
	}
	if err := out.Chmod(mode); err != nil {
		out.Close()
		os.Remove(out.Name())
		return "", err
	}
	if err := out.Close(); err != nil {
		os.Remove(out.Name())
		return "", err
	}
	return out.Name(), nil
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
