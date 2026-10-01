package update

import (
	"os"
	"path/filepath"
	"testing"
)

func TestNewer(t *testing.T) {
	for _, c := range []struct {
		a, b string
		want bool
	}{
		{"v0.2.0", "v0.1.9", true},
		{"v0.10.0", "v0.9.0", true},
		{"v0.2.0", "v0.2.0", false},
		{"v0.1.1", "v0.2.0", false},
		{"v1.0.0", "v0.99.99", true},
		{"v1.2.10-rc1", "v1.2.9", true},
		{"v1.2.1", "v1.2.3-rc1", false},
		{"v1.2.3", "v1.2.3-rc1", true},
		{"v1.2.3-rc2", "v1.2.3", false},
	} {
		if got := Newer(c.a, c.b); got != c.want {
			t.Errorf("Newer(%s, %s) = %v", c.a, c.b, got)
		}
	}
}

// TestReleaseSignature checks the latest release against the built-in key
// and its version, and that one changed byte breaks the signature. Needs
// the network: BRUMM_NET_TEST=1.
func TestReleaseSignature(t *testing.T) {
	if os.Getenv("BRUMM_NET_TEST") == "" {
		t.Skip("set BRUMM_NET_TEST=1 to check a published release")
	}
	tag, err := Latest()
	if err != nil {
		t.Fatal(err)
	}
	base := "https://github.com/" + repo + "/releases/download/" + tag + "/"
	if _, err := checksum(base+sums, tag); err != nil {
		t.Fatalf("latest release %s rejected: %v", tag, err)
	}
	list, err := fetch(base + sums)
	if err != nil {
		t.Fatal(err)
	}
	sig, err := fetch(base + sums + ".sig")
	if err != nil {
		t.Fatal(err)
	}
	if err := verify(list, sig); err != nil {
		t.Fatalf("genuine release rejected: %v", err)
	}
	list[0] ^= 1
	if verify(list, sig) == nil {
		t.Fatal("tampered checksums accepted")
	}
}

// A release with the window installs it next to brumm; one without it
// still installs.
func TestInstallWindow(t *testing.T) {
	for _, withGUI := range []bool{true, false} {
		t.Setenv("HOME", t.TempDir())
		t.Setenv("PATH", "") // no systemctl, no omarchy: nothing outside the test home
		from := t.TempDir()
		files := []string{"brumm", "omarchy/brumm.service", "omarchy/brumm.desktop", "omarchy/plugin/manifest.json"}
		if withGUI {
			files = append(files, "brumm-gui")
		}
		for _, f := range files {
			p := filepath.Join(from, f)
			if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(p, []byte(f), 0o755); err != nil {
				t.Fatal(err)
			}
		}
		if err := Install(from); err != nil {
			t.Fatal(err)
		}
		if b, err := os.ReadFile(BinPath()); err != nil || string(b) != "brumm" {
			t.Fatalf("brumm: %q %v", b, err)
		}
		fi, err := os.Stat(guiPath())
		if withGUI && (err != nil || fi.Mode()&0o111 == 0) {
			t.Fatalf("brumm-gui not installed: %v", err)
		}
		if !withGUI && err == nil {
			t.Fatal("brumm-gui from nowhere")
		}
	}
}

func TestPoints(t *testing.T) {
	body := "# brumm 1\n\n![x](y.webp)\n\n## New\n- 🪟 **A window** — `brumm --gui`, <kbd>g</kbd> switches\n- see [0.7](https://example.com)\n\n**Update:** press U."
	got := Points(body)
	want := []string{"# New", "🪟 A window — brumm --gui, g switches", "see 0.7"}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] || got[2] != want[2] {
		t.Fatalf("Points = %q, want %q", got, want)
	}
}
