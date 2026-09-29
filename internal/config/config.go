// Package config holds brumm's paths and credentials.
package config

import (
	"encoding/base64"
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// DevToken is the Apple Music developer token (an ES256 JWT, valid at most
// six months), embedded at build time:
//
//	-ldflags "-X github.com/chriopter/brumm/internal/config.DevToken=<jwt>"
//
// A developer_token in the config file takes precedence, so a local token
// can be rotated without a rebuild.
var DevToken string

type Config struct {
	DeveloperToken string `json:"developer_token,omitempty"`
	UserToken      string `json:"user_token,omitempty"`
}

func Dir() string {
	if d := os.Getenv("XDG_CONFIG_HOME"); d != "" {
		return filepath.Join(d, "brumm")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config", "brumm")
}

func CacheDir() string {
	if d := os.Getenv("XDG_CACHE_HOME"); d != "" {
		return filepath.Join(d, "brumm")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".cache", "brumm")
}

// Socket is the daemon's control socket.
func Socket() string {
	dir := os.Getenv("XDG_RUNTIME_DIR")
	if dir == "" {
		// Not the shared /tmp, where another user could claim the name.
		dir = CacheDir()
	}
	return filepath.Join(dir, "brumm.sock")
}

func path() string { return filepath.Join(Dir(), "config.json") }

// Load returns the config; a missing file is an empty config, not an error.
func Load() (Config, error) {
	var c Config
	b, err := os.ReadFile(path())
	if errors.Is(err, fs.ErrNotExist) {
		return c, nil
	}
	if err != nil {
		return c, err
	}
	err = json.Unmarshal(b, &c)
	return c, err
}

// Save writes the config with owner-only permissions: it holds the user's
// Apple Music session.
func (c Config) Save() error {
	if err := os.MkdirAll(Dir(), 0o700); err != nil {
		return err
	}
	_ = os.Chmod(Dir(), 0o700) // made by something else first, it may be open to others
	b, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	tmp := path() + ".tmp"
	if err := os.WriteFile(tmp, append(b, '\n'), 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, path())
}

// Developer returns the effective developer token.
func (c Config) Developer() string {
	if c.DeveloperToken != "" {
		return c.DeveloperToken
	}
	return DevToken
}

// Expires is when the developer token stops working (zero if unknown).
func (c Config) Expires() time.Time {
	parts := strings.Split(c.Developer(), ".")
	if len(parts) != 3 {
		return time.Time{}
	}
	raw, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return time.Time{}
	}
	var claims struct {
		Exp int64 `json:"exp"`
	}
	if json.Unmarshal(raw, &claims) != nil || claims.Exp == 0 {
		return time.Time{}
	}
	return time.Unix(claims.Exp, 0)
}
