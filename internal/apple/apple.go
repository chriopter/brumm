// Package apple is a minimal Apple Music REST client: the user's library
// and the catalog search.
package apple

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

const base = "https://api.music.apple.com"

// ErrUnauthorized means Apple rejected the session; the user must log in again.
var ErrUnauthorized = errors.New("apple music: not signed in")

// Kinds of containers.
const (
	KindPlaylist = "playlist"
	KindAlbum    = "album"
	KindArtist   = "artist"
)

// Item is a container the user can open: a playlist, an album or an artist.
// Catalog items (from search) carry Catalog so their children come from the
// catalog instead of the library.
type Item struct {
	Kind    string `json:"kind"`
	ID      string `json:"id"`
	Name    string `json:"name"`
	Artist  string `json:"artist,omitempty"`
	Catalog bool   `json:"catalog,omitempty"`
}

// Key identifies an item across kinds and sources.
func (it Item) Key() string {
	src := "lib"
	if it.Catalog {
		src = "cat"
	}
	return src + ":" + it.Kind + ":" + it.ID
}

// Track is one playable song. ID is what MusicKit plays: a library id
// ("i.…") or a catalog id.
type Track struct {
	ID       string  `json:"id"`
	Title    string  `json:"title"`
	Artist   string  `json:"artist"`
	Album    string  `json:"album"`
	Duration float64 `json:"duration"` // seconds
}

type Client struct {
	dev, user string
	http      *http.Client

	sfOnce     sync.Once
	storefront string
}

func New(developerToken, userToken string) *Client {
	return &Client{dev: developerToken, user: userToken, http: &http.Client{Timeout: 15 * time.Second}}
}

type resource struct {
	ID         string `json:"id"`
	Type       string `json:"type"`
	Attributes struct {
		Name        string `json:"name"`
		ArtistName  string `json:"artistName"`
		AlbumName   string `json:"albumName"`
		CuratorName string `json:"curatorName"`
		DurationMS  int64  `json:"durationInMillis"`
		PlayParams  *struct {
			ID string `json:"id"`
		} `json:"playParams"`
	} `json:"attributes"`
}

type page struct {
	Next string     `json:"next"`
	Data []resource `json:"data"`
}

// get fetches one document into out. Apple answers 404 for an empty
// library collection, which is reported as found=false, not an error.
func (c *Client) get(path string, out any) (found bool, err error) {
	req, err := http.NewRequest(http.MethodGet, base+path, nil)
	if err != nil {
		return false, err
	}
	req.Header.Set("Authorization", "Bearer "+c.dev)
	req.Header.Set("Music-User-Token", c.user)
	resp, err := c.http.Do(req)
	if err != nil {
		return false, err
	}
	defer resp.Body.Close()
	switch {
	case resp.StatusCode == http.StatusNotFound:
		return false, nil
	case resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
		return false, ErrUnauthorized
	case resp.StatusCode != http.StatusOK:
		return false, fmt.Errorf("apple music: %s", resp.Status)
	}
	return true, json.NewDecoder(resp.Body).Decode(out)
}

// all follows `next` links. They come back as "/v1/me/…", already carrying
// the version prefix; joining them onto a base that has it too silently
// stopped listings at the first 100 items in vibez.
func (c *Client) all(path string, maxPages int) ([]resource, error) {
	var out []resource
	for i := 0; path != "" && i < maxPages; i++ {
		var p page
		found, err := c.get(path, &p)
		if err != nil {
			if len(out) > 0 && !errors.Is(err, ErrUnauthorized) {
				return out, nil // keep what we have rather than lose the list
			}
			return out, err
		}
		if !found {
			break
		}
		out = append(out, p.Data...)
		path = p.Next
		if path != "" && !strings.HasPrefix(path, "/v1/") {
			path = "/v1/" + strings.TrimPrefix(path, "/")
		}
	}
	return out, nil
}

// Storefront is the user's country, needed for catalog requests.
func (c *Client) Storefront() string {
	c.sfOnce.Do(func() {
		var p page
		if found, err := c.get("/v1/me/storefront", &p); err == nil && found && len(p.Data) > 0 {
			c.storefront = p.Data[0].ID
		}
		if c.storefront == "" {
			c.storefront = "us"
		}
	})
	return c.storefront
}

func items(rs []resource, kind string, catalog bool) []Item {
	out := make([]Item, 0, len(rs))
	for _, r := range rs {
		a := r.Attributes
		artist := a.ArtistName
		if kind == KindPlaylist {
			artist = a.CuratorName
		}
		out = append(out, Item{Kind: kind, ID: r.ID, Name: a.Name, Artist: artist, Catalog: catalog})
	}
	return out
}

func tracks(rs []resource) []Track {
	out := make([]Track, 0, len(rs))
	for _, r := range rs {
		if r.Type != "" && r.Type != "songs" && r.Type != "library-songs" {
			continue // music videos and the like do not play as audio
		}
		a := r.Attributes
		id := r.ID
		if a.PlayParams != nil && a.PlayParams.ID != "" {
			id = a.PlayParams.ID
		}
		out = append(out, Track{ID: id, Title: a.Name, Artist: a.ArtistName, Album: a.AlbumName, Duration: float64(a.DurationMS) / 1000})
	}
	return out
}

// Library listings.

func (c *Client) Playlists() ([]Item, error) {
	rs, err := c.all("/v1/me/library/playlists?limit=100", 20)
	return items(rs, KindPlaylist, false), err
}

func (c *Client) Albums() ([]Item, error) {
	rs, err := c.all("/v1/me/library/albums?limit=100", 100)
	return items(rs, KindAlbum, false), err
}

func (c *Client) Artists() ([]Item, error) {
	rs, err := c.all("/v1/me/library/artists?limit=100", 100)
	return items(rs, KindArtist, false), err
}

func (c *Client) Songs() ([]Track, error) {
	rs, err := c.all("/v1/me/library/songs?limit=100", 300)
	return tracks(rs), err
}

// Tracks lists a playlist's or an album's songs.
func (c *Client) Tracks(it Item) ([]Track, error) {
	id := url.PathEscape(it.ID)
	var path string
	switch {
	case it.Catalog && it.Kind == KindPlaylist:
		path = "/v1/catalog/" + c.Storefront() + "/playlists/" + id + "/tracks?limit=100"
	case it.Catalog:
		path = "/v1/catalog/" + c.Storefront() + "/albums/" + id + "/tracks?limit=100"
	case it.Kind == KindPlaylist:
		path = "/v1/me/library/playlists/" + id + "/tracks?limit=100"
	default:
		path = "/v1/me/library/albums/" + id + "/tracks?limit=100"
	}
	rs, err := c.all(path, 50)
	return tracks(rs), err
}

// ArtistAlbums lists an artist's albums.
func (c *Client) ArtistAlbums(it Item) ([]Item, error) {
	id := url.PathEscape(it.ID)
	path := "/v1/me/library/artists/" + id + "/albums?limit=100"
	if it.Catalog {
		path = "/v1/catalog/" + c.Storefront() + "/artists/" + id + "/albums?limit=100"
	}
	rs, err := c.all(path, 10)
	return items(rs, KindAlbum, it.Catalog), err
}

// AlbumOf finds the album a song belongs to: in the library for library
// songs, in the catalog otherwise.
func (c *Client) AlbumOf(songID string) (Item, error) {
	id := url.PathEscape(songID)
	catalog := !strings.HasPrefix(songID, "i.")
	path := "/v1/me/library/songs/" + id + "/albums"
	if catalog {
		path = "/v1/catalog/" + c.Storefront() + "/songs/" + id + "/albums"
	}
	var p page
	found, err := c.get(path, &p)
	if err != nil {
		return Item{}, err
	}
	if !found || len(p.Data) == 0 {
		return Item{}, errors.New("no album for this song")
	}
	return items(p.Data[:1], KindAlbum, catalog)[0], nil
}

// Results are a catalog search's hits.
type Results struct {
	Songs     []Track `json:"songs"`
	Albums    []Item  `json:"albums"`
	Artists   []Item  `json:"artists"`
	Playlists []Item  `json:"playlists"`
}

func (c *Client) Search(term string) (Results, error) {
	var doc struct {
		Results map[string]page `json:"results"`
	}
	q := url.Values{"term": {term}, "types": {"songs,albums,artists,playlists"}, "limit": {"15"}}
	_, err := c.get("/v1/catalog/"+c.Storefront()+"/search?"+q.Encode(), &doc)
	if err != nil {
		return Results{}, err
	}
	return Results{
		Songs:     tracks(doc.Results["songs"].Data),
		Albums:    items(doc.Results["albums"].Data, KindAlbum, true),
		Artists:   items(doc.Results["artists"].Data, KindArtist, true),
		Playlists: items(doc.Results["playlists"].Data, KindPlaylist, true),
	}, nil
}
