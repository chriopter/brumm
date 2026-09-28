// Package apple is a minimal Apple Music REST client for the user's library.
package apple

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"
)

const base = "https://api.music.apple.com"

// ErrUnauthorized means Apple rejected the session; the user must log in again.
var ErrUnauthorized = errors.New("apple music: not signed in")

type Playlist struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

type Track struct {
	Title    string  `json:"title"`
	Artist   string  `json:"artist"`
	Album    string  `json:"album"`
	Duration float64 `json:"duration"` // seconds
}

type Client struct {
	dev, user string
	http      *http.Client
}

func New(developerToken, userToken string) *Client {
	return &Client{dev: developerToken, user: userToken, http: &http.Client{Timeout: 15 * time.Second}}
}

type resource struct {
	ID         string `json:"id"`
	Attributes struct {
		Name       string `json:"name"`
		ArtistName string `json:"artistName"`
		AlbumName  string `json:"albumName"`
		DurationMS int64  `json:"durationInMillis"`
	} `json:"attributes"`
}

type page struct {
	Next string     `json:"next"`
	Data []resource `json:"data"`
}

// get fetches one page. Apple answers 404 for an empty library collection,
// which is reported as an empty page.
func (c *Client) get(path string) (page, error) {
	var p page
	req, err := http.NewRequest(http.MethodGet, base+path, nil)
	if err != nil {
		return p, err
	}
	req.Header.Set("Authorization", "Bearer "+c.dev)
	req.Header.Set("Music-User-Token", c.user)
	resp, err := c.http.Do(req)
	if err != nil {
		return p, err
	}
	defer resp.Body.Close()
	switch {
	case resp.StatusCode == http.StatusNotFound:
		return p, nil
	case resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
		return p, ErrUnauthorized
	case resp.StatusCode != http.StatusOK:
		return p, fmt.Errorf("apple music: %s", resp.Status)
	}
	err = json.NewDecoder(resp.Body).Decode(&p)
	return p, err
}

// all follows `next` links. They come back as "/v1/me/…", i.e. already
// carrying the version prefix; joining them onto a base that has it too
// silently stopped listings at the first 100 items in vibez.
func (c *Client) all(path string, maxPages int) ([]resource, error) {
	var out []resource
	for i := 0; path != "" && i < maxPages; i++ {
		p, err := c.get(path)
		if err != nil {
			if len(out) > 0 && !errors.Is(err, ErrUnauthorized) {
				return out, nil // keep what we have rather than lose the list
			}
			return out, err
		}
		out = append(out, p.Data...)
		path = p.Next
		if path != "" && !strings.HasPrefix(path, "/v1/") {
			path = "/v1/" + strings.TrimPrefix(path, "/")
		}
	}
	return out, nil
}

func (c *Client) Playlists() ([]Playlist, error) {
	rs, err := c.all("/v1/me/library/playlists?limit=100", 20)
	out := make([]Playlist, 0, len(rs))
	for _, r := range rs {
		out = append(out, Playlist{ID: r.ID, Name: r.Attributes.Name})
	}
	return out, err
}

func (c *Client) Tracks(playlistID string) ([]Track, error) {
	rs, err := c.all("/v1/me/library/playlists/"+url.PathEscape(playlistID)+"/tracks?limit=100", 50)
	out := make([]Track, 0, len(rs))
	for _, r := range rs {
		a := r.Attributes
		out = append(out, Track{Title: a.Name, Artist: a.ArtistName, Album: a.AlbumName, Duration: float64(a.DurationMS) / 1000})
	}
	return out, err
}
