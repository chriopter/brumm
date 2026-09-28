// devtoken signs an Apple Music developer token (ES256 JWT) from a MusicKit
// key and prints it. The key never touches disk: pass its PEM through the
// environment, e.g. straight from 1Password.
//
//	APPLE_KEY_ID=… APPLE_TEAM_ID=… APPLE_PRIVATE_KEY="$(op read …)" go run ./tools/devtoken
package main

import (
	"crypto/ecdsa"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"os"
	"time"
)

// Apple rejects developer tokens that live longer than six months.
const ttl = 15777000 * time.Second

func main() {
	tok, err := sign(os.Getenv("APPLE_KEY_ID"), os.Getenv("APPLE_TEAM_ID"), os.Getenv("APPLE_PRIVATE_KEY"), time.Now())
	if err != nil {
		fmt.Fprintln(os.Stderr, "devtoken:", err)
		os.Exit(1)
	}
	fmt.Println(tok)
}

func sign(keyID, teamID, keyPEM string, now time.Time) (string, error) {
	if keyID == "" || teamID == "" || keyPEM == "" {
		return "", errors.New("APPLE_KEY_ID, APPLE_TEAM_ID and APPLE_PRIVATE_KEY are required")
	}
	block, _ := pem.Decode([]byte(keyPEM))
	if block == nil {
		return "", errors.New("APPLE_PRIVATE_KEY is not PEM")
	}
	parsed, err := x509.ParsePKCS8PrivateKey(block.Bytes)
	if err != nil {
		return "", err
	}
	key, ok := parsed.(*ecdsa.PrivateKey)
	if !ok {
		return "", errors.New("APPLE_PRIVATE_KEY is not an EC key")
	}

	enc := func(v any) string {
		b, _ := json.Marshal(v)
		return base64.RawURLEncoding.EncodeToString(b)
	}
	unsigned := enc(map[string]string{"alg": "ES256", "kid": keyID}) + "." +
		enc(map[string]any{"iss": teamID, "iat": now.Unix(), "exp": now.Add(ttl).Unix()})

	digest := sha256.Sum256([]byte(unsigned))
	r, s, err := ecdsa.Sign(rand.Reader, key, digest[:])
	if err != nil {
		return "", err
	}
	// JWS wants the raw 32-byte r and s, not ASN.1.
	sig := make([]byte, 64)
	r.FillBytes(sig[:32])
	s.FillBytes(sig[32:])
	return unsigned + "." + base64.RawURLEncoding.EncodeToString(sig), nil
}
