package update

import (
	"os"
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
	} {
		if got := Newer(c.a, c.b); got != c.want {
			t.Errorf("Newer(%s, %s) = %v", c.a, c.b, got)
		}
	}
}

// TestReleaseSignature checks a real release against the built-in key, and
// that one changed byte breaks it. Needs the network: BRUMM_NET_TEST=1.
func TestReleaseSignature(t *testing.T) {
	if os.Getenv("BRUMM_NET_TEST") == "" {
		t.Skip("set BRUMM_NET_TEST=1 to check a published release")
	}
	base := "https://github.com/" + repo + "/releases/download/v0.2.0/"
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
