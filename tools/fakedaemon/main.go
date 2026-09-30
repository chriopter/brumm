// fakedaemon stands in for brumm's daemon: a made-up library, one song
// playing, and covers drawn on the fly, served on brumm's socket. It lets
// the players be run and looked at without Apple Music:
//
//	XDG_RUNTIME_DIR=/tmp/x go run ./tools/fakedaemon
//
// FAKE_PAUSED=1 has the song paused.
//
// bin/gui-shot uses it for screenshots of the window.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"log"
	"math"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/engine"
	"github.com/chriopter/brumm/internal/ipc"
)

var albums = [][2]string{
	{"Random Access Memories", "Daft Punk"}, {"Discovery", "Daft Punk"}, {"Currents", "Tame Impala"},
	{"In Rainbows", "Radiohead"}, {"Hurry Up, We're Dreaming", "M83"}, {"Melodrama", "Lorde"},
	{"Blonde", "Frank Ocean"}, {"Kid A", "Radiohead"}, {"An Awesome Wave", "alt-J"},
	{"Since I Left You", "The Avalanches"}, {"Dummy", "Portishead"}, {"Moon Safari", "Air"},
}

var titles = []string{"Give Life Back to Music", "The Game of Love", "Giorgio by Moroder", "Within", "Instant Crush",
	"Lose Yourself to Dance", "Touch", "Get Lucky", "Beyond", "Motherboard", "Fragments of Time", "Doin' It Right", "Contact"}

var base string // where the covers are served

func art(i int) string { return fmt.Sprintf("%s/cover/%d.png", base, i) }

func tracks(a int) []apple.Track {
	var out []apple.Track
	for k, t := range titles {
		out = append(out, apple.Track{ID: fmt.Sprintf("i.%d.%d", a, k), Title: t, Artist: albums[a%len(albums)][1],
			Album: albums[a%len(albums)][0], Duration: float64(180 + (k*37)%240), Artwork: art(a), Number: k + 1})
	}
	return out
}

func items() []apple.Item {
	var out []apple.Item
	for i, a := range albums {
		out = append(out, apple.Item{Kind: apple.KindAlbum, ID: fmt.Sprintf("a%d", i), Name: a[0], Artist: a[1], Artwork: art(i),
			Info: "2013 · Columbia · 13 songs · Lossless", Note: "A love letter to the late seventies, recorded with the players who made them."})
	}
	return out
}

func playlists() []apple.Item {
	var out []apple.Item
	for i, n := range []string{"Late Night Coding", "Sunday Morning", "Favourites Mix", "Running", "Deep Focus"} {
		out = append(out, apple.Item{Kind: apple.KindPlaylist, ID: fmt.Sprintf("p%d", i), Name: n, Artwork: art(20 + i), Editable: true})
	}
	return out
}

// cover draws cover n: bands of two colors at an angle, a circle on top.
func cover(w http.ResponseWriter, r *http.Request) {
	n, _ := strconv.Atoi(strings.TrimSuffix(r.PathValue("n"), ".png"))
	hue := func(h float64) color.RGBA {
		c := func(o float64) uint8 { return uint8(127 + 127*math.Sin(2*math.Pi*(h+o))) }
		return color.RGBA{c(0), c(1.0 / 3), c(2.0 / 3), 255}
	}
	a, b := hue(float64(n)*0.137), hue(float64(n)*0.137+0.4)
	img := image.NewRGBA(image.Rect(0, 0, 300, 300))
	for y := range 300 {
		for x := range 300 {
			c := a
			if (x+y+n*17)/40%2 == 0 {
				c = b
			}
			if dx, dy := x-150, y-150; dx*dx+dy*dy < 70*70 {
				c = color.RGBA{250, 245, 235, 255}
			}
			img.Set(x, y, c)
		}
	}
	w.Header().Set("Content-Type", "image/png")
	_ = png.Encode(w, img)
}

// sound streams a made-up song at 120 beats a minute: a kick on every
// beat, a snare between, and a melody wandering over the mids.
func sound(bands, wave, fps int, stop chan struct{}, send func(ipc.Message) error) {
	tick := time.NewTicker(time.Second / time.Duration(fps))
	defer tick.Stop()
	start := time.Now()
	for {
		select {
		case <-stop:
			return
		case <-tick.C:
		}
		t := time.Since(start).Seconds()
		beat := math.Mod(t, 0.5)
		kick := math.Exp(-beat * 9)
		snare := math.Exp(-math.Mod(t+0.25, 0.5) * 12)
		m := ipc.Message{}
		for i := range bands {
			u := float64(i) / float64(max(1, bands))
			v := kick*math.Exp(-u*9)*0.95 + snare*0.35*math.Exp(-math.Abs(u-0.55)*6) +
				0.35*math.Exp(-math.Pow((u-0.3-0.15*math.Sin(t*0.7))*8, 2)) + 0.08*(1-u) + 0.05*math.Sin(t*13+u*40)
			m.Spectrum = append(m.Spectrum, int(math.Max(0, math.Min(1, v))*255))
		}
		for i := range wave {
			x := float64(i) / float64(max(1, wave))
			v := 0.6*kick*math.Sin(x*2*math.Pi*3+t*40) + 0.3*math.Sin(x*2*math.Pi*9+t*7) + 0.15*snare*math.Sin(x*2*math.Pi*31)
			m.Wave = append(m.Wave, int(math.Max(-1, math.Min(1, v))*100))
		}
		if send(m) != nil {
			return
		}
	}
}

func main() {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		log.Fatal(err)
	}
	base = "http://" + ln.Addr().String()
	mux := http.NewServeMux()
	mux.HandleFunc("GET /cover/{n}", cover)
	go func() { log.Fatal(http.Serve(ln, mux)) }()

	sock := config.Socket()
	_ = os.Remove(sock)
	l, err := net.Listen("unix", sock)
	if err != nil {
		log.Fatal(err)
	}
	log.Printf("fake brumm daemon on %s", sock)
	// FAKE_PAUSED=1: the song is paused, for measuring an idle player.
	playing := os.Getenv("FAKE_PAUSED") == ""
	st := ipc.State{Status: ipc.StatusReady, State: engine.State{Ready: true, Playing: playing, Title: "Instant Crush", Artist: "Daft Punk",
		Album: "Random Access Memories", Artwork: art(0), Pos: 97, Dur: 337, Index: 4, Length: 13, ID: "i.0.4",
		Source: "lib:album:a0", Volume: 0.8, Repeat: 2}}
	var mu sync.Mutex
	var opts config.Options
	var place json.RawMessage
	for {
		c, err := l.Accept()
		if err != nil {
			log.Fatal(err)
		}
		go func() {
			defer c.Close()
			var wmu sync.Mutex // replies and the sound share the connection
			enc, sc := json.NewEncoder(c), bufio.NewScanner(c)
			send := func(m ipc.Message) error {
				wmu.Lock()
				defer wmu.Unlock()
				return enc.Encode(m)
			}
			var stop chan struct{}
			defer func() {
				if stop != nil {
					close(stop)
				}
			}()
			for sc.Scan() {
				var r ipc.Request
				if json.Unmarshal(sc.Bytes(), &r) != nil {
					continue
				}
				m := ipc.Message{ID: r.ID}
				switch r.Cmd {
				case ipc.CmdSubscribe:
					m.State = &st
					if stop != nil {
						close(stop)
						stop = nil
					}
					if r.Bands > 0 || r.Wave > 0 {
						stop = make(chan struct{})
						go sound(r.Bands, r.Wave, max(r.FPS, 25), stop, send)
					}
				case ipc.CmdHome:
					m.Shelves = []apple.Shelf{{Title: "Recently Played", Items: items()[:6]}, {Title: "Made for You", Items: playlists()[:3]},
						{Title: "Top Songs", Tracks: tracks(3)[:6]}}
				case ipc.CmdList:
					switch r.List {
					case ipc.ListAlbums:
						m.Items = items()
					case ipc.ListPlaylists:
						m.Items = playlists()
					case ipc.ListArtists:
						for i, a := range albums {
							m.Items = append(m.Items, apple.Item{Kind: apple.KindArtist, ID: fmt.Sprintf("r%d", i), Name: a[1]})
						}
					default:
						for i := range 40 {
							m.Tracks = append(m.Tracks, tracks(i)...)
						}
					}
				case ipc.CmdOpen:
					n, _ := strconv.Atoi(strings.TrimLeft(r.Item.ID, "apr"))
					m.Tracks = tracks(n)
				case ipc.CmdSearch:
					m.Shelves = []apple.Shelf{{Title: "Top Results", Items: items()[:3]}, {Title: "Songs", Tracks: tracks(1)[:5]}}
				case ipc.CmdQueue: // the song playing, then songs from all over
					m.Tracks = tracks(0)[4:5]
					for i := 1; i < 12; i++ {
						m.Tracks = append(m.Tracks, tracks(i)[i%13])
					}
				case ipc.CmdRatings:
					m.Ratings = map[string]int{"i.0.7": 1}
				case ipc.CmdPlace: // where a player left off, as the daemon keeps it
					mu.Lock()
					if len(r.Place) > 0 {
						place = append(json.RawMessage(nil), r.Place...)
					}
					m.Place = place
					mu.Unlock()
				case ipc.CmdOptions:
					mu.Lock()
					if len(r.Options) > 0 {
						b, _ := json.Marshal(r.Options)
						_ = json.Unmarshal(b, &opts)
					}
					o := opts
					mu.Unlock()
					m.Options = &o
				}
				if send(m) != nil {
					return
				}
			}
		}()
	}
}
