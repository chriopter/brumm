package main

import (
	"fmt"
	"net/url"
	"os"
	"strings"
	"sync"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

// The API preview is deliberately a separate fixture adapter. It never
// claims fixture requests succeeded against the user's Apple account.
var preview struct {
	sync.Mutex
	favorites map[string]bool
	folders   []apple.Item
	playlists []apple.Item
}

func exploreLink(name, kind, route string) apple.Item {
	if route == "app:videos" {
		kind = apple.KindShelf
	}
	return apple.Item{ID: route, Name: name, Kind: kind, Route: route, Catalog: true}
}
func previewRoot() []apple.Item {
	return []apple.Item{
		exploreLink("Favorites", "favorites", "app:favorites"),
		exploreLink("Your Replay", "replay", "/v1/me/music-summaries?filter[year]=latest"),
	}
}
func previewTracks(a int) []apple.Track {
	ts := tracks(a)
	for i := range ts {
		ts[i].Favorite = preview.favorites[ts[i].ID]
		ts[i].Details = []apple.Detail{{Label: "Artist", Value: ts[i].Artist}, {Label: "Album", Value: ts[i].Album}, {Label: "Composer", Value: "Thomas Bangalter, Guy-Manuel de Homem-Christo"}, {Label: "Genres", Value: "Electronic, Pop"}, {Label: "Disc", Value: "1"}, {Label: "Track", Value: fmt.Sprint(i + 1)}, {Label: "ISRC", Value: fmt.Sprintf("USQX91300%03d", i)}, {Label: "Content rating", Value: "Clean"}, {Label: "Audio variants", Value: "Lossless, Dolby Atmos (catalog availability)"}, {Label: "Apple Digital Master", Value: "Yes"}, {Label: "Lyrics available", Value: "Yes; lyric text is not supplied by the public API"}, {Label: "Favorite", Value: fmt.Sprint(ts[i].Favorite)}}
	}
	return ts
}
func previewVideos(a int) []apple.Item {
	out := []apple.Item{}
	for i, name := range []string{"Instant Crush", "Lose Yourself to Dance", "Get Lucky"} {
		v := exploreLink(name, "music-videos", fmt.Sprintf("app:video:%d", i))
		v.Artist = "Daft Punk"
		v.Artwork = art(a + i)
		v.Info = "Music video · HD"
		v.PreviewURL = os.Getenv("BRUMM_PROTOTYPE_VIDEO")
		v.Details = []apple.Detail{{Label: "Artist", Value: v.Artist}, {Label: "Format", Value: "Music video"}, {Label: "Duration", Value: "5:37"}, {Label: "Playback", Value: "The prototype previews video layout. Apple DRM video integration remains separate."}}
		out = append(out, v)
	}
	return out
}
func previewExplore(r ipc.Request, m *ipc.Message) {
	preview.Lock()
	defer preview.Unlock()
	if preview.favorites == nil {
		preview.favorites = map[string]bool{"i.0.4": true, "i.0.7": true}
	}
	p := apple.ExplorePage{}
	route := r.Query
	switch {
	case route == "app:discover":
		p.Title = "Explore"
		p.Shelves = []apple.Shelf{{Title: "Browse", Items: []apple.Item{
			exploreLink("Genres", "genres", "/v1/catalog/{storefront}/genres"),
			exploreLink("Charts", "charts", "app:charts"),
			exploreLink("Music videos", "music-videos", "app:videos"),
			exploreLink("Activities", "activities", "app:activities"),
			exploreLink("Curators", "curators", "app:curators"),
			exploreLink("Record labels", "record-labels", "app:labels"),
		}}, {Title: "Find a release", Items: []apple.Item{exploreLink("Catalog lookup", "lookup", "app:lookup")}}}
	case route == "app:favorites":
		p.Title = "Favorites"
		ts := previewTracks(0)
		for _, t := range ts {
			if t.Favorite {
				p.Tracks = append(p.Tracks, t)
			}
		}
		p.Shelves = []apple.Shelf{{Title: "Albums", Items: items()[:2]}, {Title: "Artists", Items: []apple.Item{{Kind: apple.KindArtist, ID: "r0", Name: "Daft Punk"}}}, {Title: "Playlists", Items: playlists()[:1]}}
	case strings.Contains(route, "music-summaries"):
		p.Title = "Your Replay · 2025"
		p.Info = "Your most-played music of the year"
		p.Details = []apple.Detail{{Label: "Year", Value: "2025"}, {Label: "Period", Value: "Year"}, {Label: "Availability", Value: "Latest year eligible for Replay"}}
		p.Shelves = []apple.Shelf{{Title: "Top songs", Tracks: previewTracks(0)[4:9]}, {Title: "Top albums", Items: items()[:4]}, {Title: "Top artists", Items: []apple.Item{{Kind: apple.KindArtist, ID: "r0", Name: "Daft Punk"}, {Kind: apple.KindArtist, ID: "r3", Name: "Radiohead"}, {Kind: apple.KindArtist, ID: "r2", Name: "Tame Impala"}}}}
	case route == "app:charts":
		p.Items = []apple.Item{exploreLink("Top songs", "charts", "app:chart:songs"), exploreLink("Top albums", "charts", "app:chart:albums"), exploreLink("Global top songs", "charts", "app:chart:global"), exploreLink("City charts", "charts", "app:cities"), exploreLink("Charts by genre", "genres", "/v1/catalog/{storefront}/genres"), exploreLink("Top music videos", "music-videos", "app:videos")}
	case route == "app:cities":
		for _, city := range []string{"Berlin", "London", "Tokyo", "New York"} {
			p.Items = append(p.Items, exploreLink(city, "charts", "app:chart:city:"+city))
		}
	case strings.HasPrefix(route, "app:chart:"):
		if strings.Contains(route, "albums") {
			p.Items = items()
		} else {
			p.Tracks = previewTracks(3)[:8]
		}
		p.Next = "app:more:" + url.QueryEscape(route)
	case strings.Contains(route, "/genres") && !strings.Contains(route, "/station-genres"):
		if strings.HasSuffix(route, "/genres") {
			for _, g := range []string{"Electronic", "Alternative", "Jazz", "Classical", "Pop", "R&B"} {
				p.Items = append(p.Items, exploreLink(g, "genres", "app:genre:"+g))
			}
		} else {
			p.Items = items()[:4]
		}
	case strings.HasPrefix(route, "app:genre:"):
		name := strings.TrimPrefix(route, "app:genre:")
		p.Title = name
		p.Shelves = []apple.Shelf{{Title: "Albums", Items: items()[:4]}, {Title: "Songs", Tracks: previewTracks(2)[:5]}, {Title: "More", Items: []apple.Item{exploreLink(name+" charts", "charts", "app:chart:genre:"+name), exploreLink(name+" radio", "station-genres", "app:radio:"+name)}}}
	case route == "app:radio-genres" || strings.Contains(route, "/station-genres"):
		for _, g := range []string{"Electronic", "Alternative", "Jazz", "Classical", "Pop"} {
			p.Items = append(p.Items, exploreLink(g, "station-genres", "app:radio:"+g))
		}
	case strings.HasPrefix(route, "app:radio:"):
		for _, n := range []string{"Essentials Radio", "New Releases Radio", "Late Night Radio"} {
			p.Items = append(p.Items, apple.Item{ID: n, Kind: apple.KindStation, Name: n, Artist: strings.TrimPrefix(route, "app:radio:"), Catalog: true, Artwork: art(4)})
		}
	case route == "app:activities":
		for _, n := range []string{"Focus", "Workout", "Relax", "Sleep", "Party"} {
			p.Items = append(p.Items, exploreLink(n, "activities", "app:activity:"+n))
		}
	case strings.HasPrefix(route, "app:activity:"):
		p.Shelves = []apple.Shelf{{Title: "Playlists", Items: playlists()}, {Title: "Songs", Tracks: previewTracks(4)[:5]}}
	case route == "app:curators":
		for _, n := range []string{"Apple Music Electronic", "Apple Music Alternative", "Ninja Tune", "Boiler Room"} {
			p.Items = append(p.Items, exploreLink(n, "curators", "app:curator:"+n))
		}
	case strings.HasPrefix(route, "app:curator:"):
		p.Shelves = []apple.Shelf{{Title: "Playlists", Items: playlists()}, {Title: "Featured albums", Items: items()[:4]}}
	case route == "app:labels":
		for _, n := range []string{"Ninja Tune", "Warp Records", "Columbia", "XL Recordings"} {
			p.Items = append(p.Items, exploreLink(n, "record-labels", "app:label:"+n))
		}
	case strings.HasPrefix(route, "app:label:"):
		p.Shelves = []apple.Shelf{{Title: "Latest releases", Items: items()[:6]}, {Title: "Featured releases", Items: items()[6:]}}
	case route == "app:videos":
		p.Items = previewVideos(0)
	case strings.HasPrefix(route, "app:video:"):
		p.Info = "Local video preview; no Apple Music video is streamed in this prototype."
		p.Items = previewVideos(0)
	case route == "app:lookup":
		p.Items = []apple.Item{exploreLink("Find a song by ISRC", "lookup", "action:lookup:isrc"), exploreLink("Find an album by UPC", "lookup", "action:lookup:upc"), exploreLink("Find an equivalent release", "lookup", "action:lookup:equivalent"), exploreLink("Browse another country", "lookup", "action:lookup:storefront"), exploreLink("Look up multiple resources", "lookup", "action:lookup:batch")}
	case strings.HasPrefix(route, "app:lookup:"):
		p.Info = "Sample lookup results"
		p.Items = items()[:2]
		p.Tracks = previewTracks(0)[:3]
	case route == "app:folders":
		p.Items = []apple.Item{exploreLink("Coding", "folder", "app:folder:Coding"), exploreLink("Weekends", "folder", "app:folder:Weekends")}
		for _, it := range preview.folders {
			if it.Artist == route {
				p.Items = append(p.Items, it)
			}
		}
		p.Items = append(p.Items, playlists()...)
		for _, it := range preview.playlists {
			if it.Artist == route {
				p.Items = append(p.Items, it)
			}
		}
		p.Items = append(p.Items, exploreLink("New folder…", "folder", "action:create:folder"), exploreLink("New playlist…", "playlist", "action:create:playlist"))
	case strings.HasPrefix(route, "app:folder:"):
		p.Items = playlists()[:3]
		for _, it := range preview.folders {
			if it.Artist == route {
				p.Items = append(p.Items, it)
			}
		}
		for _, it := range preview.playlists {
			if it.Artist == route {
				p.Items = append(p.Items, it)
			}
		}
		p.Items = append(p.Items, exploreLink("Night sessions", "folder", route+"/Night sessions"), exploreLink("New folder…", "folder", "action:create:folder"), exploreLink("New playlist…", "playlist", "action:create:playlist"))
	case strings.HasPrefix(route, "app:recommendation:"):
		p.Items = playlists()
		p.Next = "app:more:recommendations"
	case strings.HasPrefix(route, "app:more:"):
		p.Items = items()[6:]
		p.Tracks = previewTracks(5)[6:]
	default:
		m.Error = "This route is not in the local preview library"
		return
	}
	m.Explorer = &p
}
func previewMutation(r ipc.Request, m *ipc.Message) {
	preview.Lock()
	defer preview.Unlock()
	if preview.favorites == nil {
		preview.favorites = map[string]bool{}
	}
	switch r.Cmd {
	case "preview-favorite":
		preview.favorites[r.Start] = r.Value > 0
	case "preview-create":
		name := strings.TrimSpace(r.Query)
		if name == "" {
			m.Error = "Choose a name"
			return
		}
		if r.List == "folder" {
			it := exploreLink(name, "folder", r.Source+"/"+url.PathEscape(name))
			if r.Source == "app:folders" {
				it.Route = "app:folder:" + url.PathEscape(name)
				it.ID = it.Route
			}
			it.Artist = r.Source
			preview.folders = append(preview.folders, it)
		} else {
			it := apple.Item{Kind: apple.KindPlaylist, ID: fmt.Sprintf("p%d", 100+len(preview.playlists)), Name: name, Artist: r.Source, Note: "Created in the local prototype", Editable: true}
			preview.playlists = append(preview.playlists, it)
		}
	}
}
