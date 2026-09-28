package chrome

import (
	"bytes"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestListedHash(t *testing.T) {
	release := []byte("Origin: Google LLC\nMD5Sum:\n 0123 6207 main/binary-amd64/Packages\nSHA256:\n aaaa 1411 main/binary-amd64/Packages.gz\n bbbb 6207 main/binary-amd64/Packages\n")
	if got := listedHash(release, "main/binary-amd64/Packages"); got != "bbbb" {
		t.Fatalf("got %q, want the SHA256 entry", got)
	}
	if got := listedHash(release, "main/binary-arm64/Packages"); got != "" {
		t.Fatalf("got %q for an unlisted path", got)
	}
}

func TestStanza(t *testing.T) {
	pk := []byte("Package: google-chrome-beta\nFilename: pool/beta.deb\nSHA256: 11\n\nPackage: google-chrome-stable\nVersion: 1\nFilename: pool/stable.deb\nSHA256: 22\n")
	if f, s := stanza(pk, "google-chrome-stable"); f != "pool/stable.deb" || s != "22" {
		t.Fatalf("got %q %q", f, s)
	}
}

// TestSignatureChain checks the live repository: the signed InRelease
// verifies, and one changed byte does not. BRUMM_NET=1 enables it.
func TestSignatureChain(t *testing.T) {
	if os.Getenv("BRUMM_NET") == "" {
		t.Skip("set BRUMM_NET=1 to test against dl.google.com")
	}
	in, err := get(http.DefaultClient, repo+"dists/stable/InRelease", 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	key := filepath.Join(dir, "k.gpg")
	os.WriteFile(key, googleKey, 0o600)
	verify := func(b []byte) ([]byte, error) {
		p := filepath.Join(dir, "InRelease")
		os.WriteFile(p, b, 0o600)
		return exec.Command("gpgv", "--keyring", key, "--output", "-", p).Output()
	}
	rel, err := verify(in)
	if err != nil {
		t.Fatalf("genuine InRelease rejected: %v", err)
	}
	if listedHash(rel, "main/binary-amd64/Packages") == "" {
		t.Fatal("no Packages checksum in the signed text")
	}
	bad := bytes.Replace(in, []byte("main/binary-amd64/Packages\n"), []byte("main/binary-amd64/Packagez\n"), 1)
	if _, err := verify(bad); err == nil {
		t.Fatal("tampered InRelease accepted")
	}
}
