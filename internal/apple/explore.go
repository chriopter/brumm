package apple

import (
	"encoding/json"
	"fmt"
	"net/url"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

// Detail preserves API attributes without adding a UI-specific field for
// every new metadata property. Both players can render the same document.
type Detail struct {
	Label string `json:"label"`
	Value string `json:"value"`
}
type ExplorePage struct {
	Title   string   `json:"title,omitempty"`
	Info    string   `json:"info,omitempty"`
	Details []Detail `json:"details,omitempty"`
	Shelves []Shelf  `json:"shelves,omitempty"`
	Items   []Item   `json:"items,omitempty"`
	Tracks  []Track  `json:"tracks,omitempty"`
	Next    string   `json:"next,omitempty"`
}
type exploreResource struct {
	ID            string                       `json:"id"`
	Type          string                       `json:"type"`
	Href          string                       `json:"href"`
	Attributes    map[string]any               `json:"attributes"`
	Relationships map[string]exploreCollection `json:"relationships"`
	Views         map[string]exploreCollection `json:"views"`
}
type exploreCollection struct {
	Data       []exploreResource `json:"data"`
	Href       string            `json:"href"`
	Next       string            `json:"next"`
	Attributes map[string]any    `json:"attributes"`
	Name       string            `json:"name"`
}

// Explore fetches one page; the clients follow Next on demand. Every
// server-provided href is validated before a token-bearing request is made.
func (c *Client) Explore(route string) (ExplorePage, error) {
	var out ExplorePage
	if err := validExploreRoute(route); err != nil {
		return out, err
	}
	var doc struct {
		Data []exploreResource `json:"data"`
		Next string            `json:"next"`
		Meta struct {
			Filters map[string]map[string][]exploreResource `json:"filters"`
		} `json:"meta"`
		Results map[string]json.RawMessage `json:"results"`
	}
	found, err := c.get(route, &doc)
	if err != nil {
		return out, err
	}
	if !found {
		return out, fmt.Errorf("Apple Music has no resource at this route")
	}
	if len(doc.Data) == 0 {
		for _, filter := range doc.Meta.Filters {
			for _, resources := range filter {
				doc.Data = append(doc.Data, resources...)
			}
		}
	}
	if err := c.hydrateSparse(doc.Data); err != nil {
		return out, err
	}
	out.Next = doc.Next
	out.Items, out.Tracks = exploreMixed(doc.Data)
	if len(doc.Data) == 1 {
		r := doc.Data[0]
		out.Title = attributeString(r.Attributes, "name")
		out.Details = attributeDetails(r.Attributes)
		if len(r.Relationships)+len(r.Views) > 0 && r.Type != "songs" && r.Type != "library-songs" {
			out.Items = nil
		}
		for _, group := range []map[string]exploreCollection{r.Views, r.Relationships} {
			keys := sortedKeys(group)
			for _, name := range keys {
				coll := group[name]
				items, tracks := exploreMixed(coll.Data)
				if coll.Next != "" {
					items = append(items, Item{Kind: KindShelf, ID: coll.Next, Name: "More…", Route: coll.Next, Catalog: true})
				}
				if (len(items) == 0 && len(tracks) == 0 || sparseResources(coll.Data)) && coll.Href != "" {
					items = []Item{{Kind: KindShelf, ID: coll.Href, Name: humanLabel(name), Route: coll.Href, Catalog: true}}
				}
				if len(items)+len(tracks) > 0 {
					out.Shelves = append(out.Shelves, Shelf{Title: collectionTitle(name, coll.Attributes), Items: items, Tracks: tracks})
				}
			}
		}
	}
	for _, name := range sortedKeys(doc.Results) {
		var single exploreCollection
		var groups []exploreCollection
		if err := json.Unmarshal(doc.Results[name], &groups); err != nil {
			if json.Unmarshal(doc.Results[name], &single) != nil {
				continue
			}
			groups = []exploreCollection{single}
		}
		for _, group := range groups {
			items, tracks := exploreMixed(group.Data)
			if group.Next != "" {
				items = append(items, Item{Kind: KindShelf, ID: group.Next, Name: "More…", Route: group.Next, Catalog: true})
			}
			title := group.Name
			if title == "" {
				title = attributeString(group.Attributes, "name")
			}
			if title == "" {
				title = humanLabel(name)
			}
			out.Shelves = append(out.Shelves, Shelf{Title: title, Items: items, Tracks: tracks})
		}
	}
	return out, nil
}

func validExploreRoute(route string) error {
	u, err := url.Parse(route)
	if err != nil || u.IsAbs() || u.Host != "" || u.Fragment != "" || strings.Contains(u.Path, "..") || strings.Contains(u.Path, "\\") || !(strings.HasPrefix(u.Path, "/v1/catalog/") || strings.HasPrefix(u.Path, "/v1/me/") || u.Path == "/v1/storefronts" || strings.HasPrefix(u.Path, "/v1/storefronts/")) {
		return fmt.Errorf("invalid public Apple Music route")
	}
	return nil
}
func exploreMixed(rs []exploreResource) ([]Item, []Track) {
	var itemsOut []Item
	var tracksOut []Track
	for _, r := range rs {
		if strings.HasSuffix(r.Type, "-period-summaries") {
			for _, relationship := range r.Relationships {
				its, ts := exploreMixed(relationship.Data)
				itemsOut = append(itemsOut, its...)
				tracksOut = append(tracksOut, ts...)
			}
			continue
		}
		raw, _ := json.Marshal(r)
		var legacy resource
		_ = json.Unmarshal(raw, &legacy)
		details := attributeDetails(r.Attributes)
		if r.Type == "songs" || r.Type == "library-songs" {
			ts := tracks([]resource{legacy})
			if len(ts) > 0 {
				if ts[0].Title == "" {
					ts[0].Title = "Unavailable song (" + r.ID + ")"
				}
				ts[0].Details = details
				ts[0].Favorite, _ = r.Attributes["inFavorites"].(bool)
				tracksOut = append(tracksOut, ts[0])
			}
			continue
		}
		kind, catalog, ok := kindOf(r.Type)
		if !ok {
			kind = strings.TrimPrefix(r.Type, "library-")
			catalog = !strings.HasPrefix(r.Type, "library-")
		}
		it := item(legacy, kind, catalog)
		it.Details = details
		if it.Name == "" {
			it.Name = humanLabel(r.Type)
			if year := attributeString(r.Attributes, "year"); year != "" {
				it.Name = "Replay " + year
			}
		}
		it.Route = r.Href
		if it.Route == "" {
			if catalog {
				it.Route = "/v1/catalog/{storefront}/" + r.Type + "/" + url.PathEscape(r.ID)
			} else {
				it.Route = "/v1/me/library/" + strings.TrimPrefix(r.Type, "library-") + "/" + url.PathEscape(r.ID)
			}
		}
		if r.Type == "storefronts" {
			it.Route = "app:country:" + r.ID
		}
		if r.Type == "genres" {
			it.Route = "app:genre:" + r.ID
		}
		if r.Type == "library-playlist-folders" {
			it.Route = strings.TrimSuffix(it.Route, "/children") + "/children"
		}
		itemsOut = append(itemsOut, it)
	}
	return itemsOut, tracksOut
}
func sortedKeys[T any](m map[string]T) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
func attributeString(a map[string]any, key string) string {
	v, ok := a[key]
	if !ok || v == nil {
		return ""
	}
	switch t := v.(type) {
	case string:
		return t
	case float64:
		return strconv.FormatFloat(t, 'f', -1, 64)
	default:
		b, _ := json.Marshal(v)
		return string(b)
	}
}
func attributeDetails(attrs map[string]any) []Detail {
	var out []Detail
	for _, key := range sortedKeys(attrs) {
		if key == "artwork" || key == "playParams" || key == "previews" {
			continue
		}
		val := attributeString(attrs, key)
		if val == "" {
			continue
		}
		if list, ok := attrs[key].([]any); ok {
			parts := []string{}
			for _, v := range list {
				parts = append(parts, fmt.Sprint(v))
			}
			val = strings.Join(parts, ", ")
		}
		if nested, ok := attrs[key].(map[string]any); ok {
			if text, ok := nested["standard"].(string); ok {
				val = plain(text)
			}
		}
		out = append(out, Detail{Label: humanLabel(key), Value: val})
	}
	return out
}
func humanLabel(s string) string {
	labels := map[string]string{"inFavorites": "Favorite", "hasLyrics": "Lyrics available", "audioVariants": "Audio variants", "composerName": "Composer", "discNumber": "Disc", "contentRating": "Content rating", "genreNames": "Genres", "isrc": "ISRC", "upc": "UPC", "isAppleDigitalMaster": "Apple Digital Master", "music-videos": "Music videos", "library-playlist-folders": "Playlist folders", "station-genres": "Radio genres", "apple-curators": "Apple curators", "record-labels": "Record labels"}
	if name := labels[s]; name != "" {
		return name
	}
	s = strings.ReplaceAll(s, "-", " ")
	s = camelLabel.ReplaceAllString(s, "$1 $2")
	if s != "" {
		s = strings.ToUpper(s[:1]) + s[1:]
	}
	return s
}

func sparseResources(rs []exploreResource) bool {
	if len(rs) == 0 {
		return false
	}
	for _, r := range rs {
		if len(r.Attributes) > 0 || len(r.Relationships) > 0 || len(r.Views) > 0 {
			return false
		}
	}
	return true
}

var camelLabel = regexp.MustCompile(`([a-z])([A-Z])`)

// Sparse relationships (notably Replay songs) contain only identifiers.
// Hydrate these in batches, rather than displaying blank playable rows.
func (c *Client) hydrateSparse(rs []exploreResource) error {
	groups := map[string][]string{}
	resolved := map[string]exploreResource{}
	var walk func([]exploreResource, func(*exploreResource))
	walk = func(resources []exploreResource, visit func(*exploreResource)) {
		for i := range resources {
			r := &resources[i]
			visit(r)
			for name, coll := range r.Relationships {
				walk(coll.Data, visit)
				r.Relationships[name] = coll
			}
			for name, coll := range r.Views {
				walk(coll.Data, visit)
				r.Views[name] = coll
			}
		}
	}
	walk(rs, func(r *exploreResource) {
		if len(r.Attributes) > 0 {
			return
		}
		switch strings.TrimPrefix(r.Type, "library-") {
		case "songs", "albums", "artists", "playlists", "music-videos", "stations":
			groups[r.Type] = append(groups[r.Type], r.ID)
		}
	})
	for _, typ := range sortedKeys(groups) {
		ids := dedupe(groups[typ])
		for len(ids) > 0 {
			n := min(50, len(ids))
			part := ids[:n]
			ids = ids[n:]
			path := c.catalogPath("/" + typ)
			if strings.HasPrefix(typ, "library-") {
				path = "/v1/me/library/" + strings.TrimPrefix(typ, "library-")
			}
			q := url.Values{"ids": {strings.Join(part, ",")}}
			var doc exploreCollection
			_, err := c.get(path+"?"+q.Encode(), &doc)
			if err != nil {
				if err == ErrUnauthorized {
					return err
				}
				continue
			}
			for _, r := range doc.Data {
				resolved[r.Type+":"+r.ID] = r
			}
		}
	}
	walk(rs, func(r *exploreResource) {
		if full, ok := resolved[r.Type+":"+r.ID]; ok && len(r.Attributes) == 0 {
			r.Attributes = full.Attributes
			if r.Href == "" {
				r.Href = full.Href
			}
		}
	})
	return nil
}

func collectionTitle(name string, attrs map[string]any) string {
	if title, ok := attrs["title"].(string); ok && title != "" {
		return title
	}
	return humanLabel(name)
}

func inheritPageQuery(previous, next string) string {
	source, err := url.Parse(previous)
	if err != nil {
		return next
	}
	target, err := url.Parse(next)
	if err != nil {
		return next
	}
	query := target.Query()
	initial := source.Query()
	for _, key := range []string{"limit", "extend", "include", "l"} {
		if !query.Has(key) && initial.Has(key) {
			query[key] = initial[key]
		}
	}
	target.RawQuery = query.Encode()
	return target.String()
}
