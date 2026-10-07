package launch

import (
	"os"
	"strings"
)

// Executable keeps the installed path usable after Linux marks the running
// executable as deleted following an atomic update.
func Executable() (string, error) {
	p, err := os.Executable()
	return strings.TrimSuffix(p, " (deleted)"), err
}

// BinaryUpdated compares the running inode with the installed program.
// Release timestamps are build times, so comparing modification dates fails.
func BinaryUpdated() bool {
	p, err := Executable()
	if err != nil {
		return false
	}
	return replaced("/proc/self/exe", p)
}

func replaced(running, installed string) bool {
	a, err := os.Stat(running)
	if err != nil {
		return false
	}
	b, err := os.Stat(installed)
	return err == nil && !os.SameFile(a, b)
}
