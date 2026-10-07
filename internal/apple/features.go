package apple

import (
	"fmt"
	"net/url"
	"strings"
)

func featureLink(name, route string) Item {
	return Item{Kind: KindShelf, ID: route, Name: name, Route: route, Catalog: true}
}

// Feature resolves named pages to public API requests. The same pages are
// consumed by both clients; only actions require a client-specific form.
func (c *Client) Feature(route string) (ExplorePage, error) {
	switch route {
	case "app:discover":
		return ExplorePage{Title: "Explore", Items: []Item{
			featureLink("Genres", c.catalogPath("/genres")),
			featureLink("Radio genres", c.catalogPath("/station-genres")),
			featureLink("Charts", "app:charts"),
			featureLink("Music videos", c.catalogPath("/charts?types=music-videos")),
			featureLink("Recommendations", "/v1/me/recommendations"),
			featureLink("Catalog lookup", "app:lookup"),
			featureLink("Countries and languages", "/v1/storefronts"),
		}, Info: "Search also finds activities, curators and record labels."}, nil
	case "app:charts":
		return ExplorePage{Title: "Charts", Items: []Item{
			featureLink("Top songs, albums and playlists", c.catalogPath("/charts?types=songs,albums,playlists")),
			featureLink("City charts", c.catalogPath("/charts?types=playlists&with=cityCharts")),
			featureLink("Global charts", c.catalogPath("/charts?types=playlists&with=dailyGlobalTopCharts")),
			featureLink("Music videos", c.catalogPath("/charts?types=music-videos")),
		}}, nil
	case "app:favorites:songs", "app:favorites:albums", "app:favorites:artists", "app:favorites:playlists":
		typ := strings.TrimPrefix(route, "app:favorites:")
		rs, err := c.all("/v1/me/library/"+typ+"?limit=100&extend=inFavorites", 300)
		if err != nil {
			return ExplorePage{}, err
		}
		var selected []resource
		for _, r := range rs {
			if r.Attributes.Extra["inFavorites"] == true {
				selected = append(selected, r)
			}
		}
		items, tracks := mixed(selected)
		return ExplorePage{Title: "Favorite " + typ, Items: items, Tracks: tracks, Info: "Shows favorites reported by Apple Music for your library."}, nil
	case "app:favorites":
		return ExplorePage{Title: "Favorites", Items: []Item{
			featureLink("Songs", "app:favorites:songs"),
			featureLink("Albums", "app:favorites:albums"),
			featureLink("Artists", "app:favorites:artists"),
			featureLink("Playlists", "app:favorites:playlists"),
		}}, nil
	case "app:replay":
		route = "/v1/me/music-summaries?filter[year]=latest&views=top-artists,top-albums,top-songs"
	case "app:folders":
		root, err := c.Explore("/v1/me/library/playlist-folders?filter[identity]=playlistsroot")
		if err != nil {
			return root, err
		}
		if len(root.Items) == 0 {
			return ExplorePage{Title: "Playlist folders", Info: "Apple Music has not provided a folder root for this account. Your playlists are available in Playlists.", Items: []Item{featureLink("New folder…", "action:create:folder"), featureLink("New playlist…", "action:create:playlist")}}, nil
		}
		route = root.Items[0].Route
	case "app:lookup":
		return ExplorePage{Title: "Catalog lookup", Items: []Item{
			featureLink("Find a song by ISRC", "action:lookup:isrc"),
			featureLink("Find an album by UPC", "action:lookup:upc"),
			featureLink("Find equivalent songs by catalog ID", "action:lookup:equivalent"),
			featureLink("Look up songs by catalog IDs", "action:lookup:songs"),
			featureLink("Look up albums by catalog IDs", "action:lookup:albums"),
			featureLink("Look up videos by catalog IDs", "action:lookup:music-videos"),
			featureLink("Mixed IDs (songs:ID; albums:ID)", "action:lookup:batch"),
		}}, nil
	default:
		if strings.HasPrefix(route, "app:country:") {
			country := strings.TrimPrefix(route, "app:country:")
			if len(country) != 2 || country[0] < 'a' || country[0] > 'z' || country[1] < 'a' || country[1] > 'z' {
				return ExplorePage{}, fmt.Errorf("invalid country")
			}
			return ExplorePage{Title: strings.ToUpper(country), Items: []Item{
				featureLink("Charts", "/v1/catalog/"+country+"/charts?types=songs,albums,playlists"),
				featureLink("Genres", "/v1/catalog/"+country+"/genres"),
				featureLink("Languages", "/v1/storefronts/"+country),
			}}, nil
		}
		if strings.HasPrefix(route, "app:genre:") {
			genre := strings.TrimPrefix(route, "app:genre:")
			return c.Explore(c.catalogPath("/charts?types=songs,albums,playlists&genre=") + url.QueryEscape(genre))
		}
		if strings.HasPrefix(route, "app:lookup:") {
			parts := strings.SplitN(strings.TrimPrefix(route, "app:lookup:"), ":", 2)
			if len(parts) != 2 || strings.TrimSpace(parts[1]) == "" {
				return ExplorePage{}, fmt.Errorf("enter an identifier")
			}
			q := url.Values{}
			switch parts[0] {
			case "isrc":
				q.Set("filter[isrc]", parts[1])
				route = c.catalogPath("/songs?") + q.Encode()
			case "upc":
				q.Set("filter[upc]", parts[1])
				route = c.catalogPath("/albums?") + q.Encode()
			case "batch":
				for _, group := range strings.Split(parts[1], ";") {
					typ, ids, ok := strings.Cut(strings.TrimSpace(group), ":")
					if !ok || strings.TrimSpace(ids) == "" {
						return ExplorePage{}, fmt.Errorf("use songs:ID; albums:ID")
					}
					switch typ {
					case "songs", "albums", "artists", "playlists", "music-videos", "stations", "activities", "curators", "apple-curators", "record-labels":
					default:
						return ExplorePage{}, fmt.Errorf("unsupported resource type")
					}
					q.Set("ids["+typ+"]", strings.TrimSpace(ids))
				}
				route = c.catalogPath("") + "?" + q.Encode()
			case "songs", "albums", "music-videos":
				q.Set("ids", parts[1])
				route = c.catalogPath("/"+parts[0]+"?") + q.Encode()
			case "equivalent":
				q.Set("filter[equivalents]", parts[1])
				route = c.catalogPath("/songs?") + q.Encode()
			default:
				return ExplorePage{}, fmt.Errorf("unknown lookup")
			}
		}
	}
	route = strings.ReplaceAll(route, "{storefront}", c.Storefront())
	page, err := c.Explore(route)
	if err != nil {
		return page, err
	}
	if len(page.Items)+len(page.Tracks)+len(page.Shelves) == 0 {
		page.Info = "No results available for this account or country."
	}
	if strings.Contains(route, "/playlist-folders/") {
		page.Items = append(page.Items, featureLink("New folder…", "action:create:folder"), featureLink("New playlist…", "action:create:playlist"))
	}
	return page, nil
}

// CreateContainer creates an empty playlist or folder, optionally inside
// an existing folder. It never emulates unsupported editing operations.
func (c *Client) CreateContainer(kind, name, parent string) (Item, error) {
	if strings.TrimSpace(name) == "" {
		return Item{}, fmt.Errorf("enter a name")
	}
	typ := "playlists"
	if kind == "folder" {
		typ = "playlist-folders"
	} else if kind != "playlist" {
		return Item{}, fmt.Errorf("unsupported container")
	}
	body := map[string]any{"attributes": map[string]string{"name": strings.TrimSpace(name)}}
	if parent != "" && parent != "p.playlistsroot" {
		body["relationships"] = map[string]any{"parent": map[string]any{"data": []map[string]string{{"id": parent, "type": "library-playlist-folders"}}}}
	}
	var doc page
	if err := c.send("POST", "/v1/me/library/"+typ, body, &doc); err != nil {
		return Item{}, err
	}
	if len(doc.Data) == 0 {
		return Item{}, fmt.Errorf("Apple Music did not return the created resource")
	}
	k := KindPlaylist
	if kind == "folder" {
		k = KindFolder
	}
	return item(doc.Data[0], k, false), nil
}

func (c *Client) Favorite(refs []Ref) error {
	if len(refs) == 0 {
		return fmt.Errorf("select a resource")
	}
	groups := map[string][]string{}
	for _, ref := range refs {
		kind := ref.Kind
		if kind == "" || kind == "track" {
			kind = "song"
		}
		switch kind {
		case "song", KindAlbum, KindArtist, KindPlaylist:
		default:
			return fmt.Errorf("this resource cannot be favorited")
		}
		typ := kind + "s"
		for _, prefix := range []string{"i.", "l.", "p.", "r."} {
			if strings.HasPrefix(ref.ID, prefix) {
				typ = "library-" + typ
				break
			}
		}
		groups[typ] = append(groups[typ], ref.ID)
	}
	q := url.Values{}
	for typ, ids := range groups {
		q.Set("ids["+typ+"]", strings.Join(dedupe(ids), ","))
	}
	return c.send("POST", "/v1/me/favorites?"+q.Encode(), nil, nil)
}
