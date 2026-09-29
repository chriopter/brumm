package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
)

// / in a list filters it where you are: the rows narrow as you type, over
// titles, artists and albums, every word having to match. tab (or enter
// with nothing left) takes the words to a search of all of Apple Music;
// esc drops the filter. The Search tab and the queue keep / for search.

// filterable is whether / filters the list on screen.
func (m *Model) filterable() bool {
	v := m.cur()
	return m.section != secSearch && v.key != "queue:" && v.loaded && len(v.rows)+len(v.all) > 0
}

// startFilter puts the cursor in the list's filter box.
func (m *Model) startFilter() {
	v := m.cur()
	if v.all == nil {
		v.all = v.rows
	}
	m.filtering, m.help = true, false
}

// applyFilter shows the rows of v that match its filter.
func applyFilter(v *view) {
	if v.all == nil {
		return
	}
	words := strings.Fields(strings.ToLower(v.filter))
	if len(words) == 0 {
		v.rows = v.all
	} else {
		v.rows = nil
		for _, r := range v.all {
			var text string
			switch {
			case r.track != nil:
				text = r.track.Title + " " + r.track.Artist + " " + r.track.Album
			case r.item != nil:
				text = r.item.Name + " " + r.item.Artist
			default:
				continue // headings and notes do not survive a filter
			}
			text = strings.ToLower(text)
			all := true
			for _, w := range words {
				if !strings.Contains(text, w) {
					all = false
					break
				}
			}
			if all {
				v.rows = append(v.rows, r)
			}
		}
	}
	v.sel, v.off = nearest(v.rows, 0, 1), 0
}

// clearFilter shows the whole list again, keeping the selected row.
func (m *Model) clearFilter(v *view) {
	if v.all == nil {
		return
	}
	var keep row
	if v.sel < len(v.rows) {
		keep = v.rows[v.sel]
	}
	v.rows, v.all, v.filter = v.all, nil, ""
	for i, r := range v.rows {
		if (keep.track != nil && r.track == keep.track) || (keep.item != nil && r.item == keep.item) {
			v.sel = i
		}
	}
	m.filtering = false
}

// searchEverywhere takes the filter's words to a search of Apple Music.
func (m *Model) searchEverywhere(q string) tea.Cmd {
	m.filtering = false
	m.clearFilter(m.cur())
	m.query = q
	m.section, m.searching = secSearch, false
	return m.runSearch(true)
}

func (m *Model) filterKey(msg tea.KeyPressMsg) tea.Cmd {
	v := m.cur()
	switch msg.String() {
	case "esc":
		m.clearFilter(v)
		return nil
	case "enter":
		if len(v.rows) == 0 && strings.TrimSpace(v.filter) != "" {
			return m.searchEverywhere(v.filter) // nothing here: look everywhere
		}
		m.filtering = false // into the rows that are left
		return m.prefetchCovers(v)
	case "tab":
		if strings.TrimSpace(v.filter) != "" {
			return m.searchEverywhere(v.filter)
		}
		return nil
	case "backspace":
		if r := []rune(v.filter); len(r) > 0 {
			v.filter = string(r[:len(r)-1])
		}
	case "ctrl+w":
		f := strings.TrimRight(v.filter, " ")
		if i := strings.LastIndex(f, " "); i >= 0 {
			v.filter = f[:i+1]
		} else {
			v.filter = ""
		}
	case "ctrl+u":
		v.filter = ""
	case "ctrl+c":
		return tea.Quit
	case "down", "up":
		m.filtering = false
		return m.move(msg.String())
	default:
		if msg.Text == "" {
			return nil
		}
		v.filter += msg.Text
	}
	applyFilter(v)
	return m.prefetchCovers(v)
}

// filterBox is the line over a filtered list.
func (m *Model) filterBox(w int) string {
	v := m.cur()
	cursor := ""
	if m.filtering {
		cursor = sHere.Render("▏")
	}
	hint := sDim.Render("  ") + sKey.Render("tab") + sDim.Render(" all of Apple Music  ") + sKey.Render("esc") + sDim.Render(" clear")
	if !m.filtering {
		hint = sDim.Render("  ") + sKey.Render("/") + sDim.Render(" edit  ") + sKey.Render("esc") + sDim.Render(" clear")
	}
	return fit(sHere.Render("󰈲")+"  "+v.filter+cursor+hint, w)
}
