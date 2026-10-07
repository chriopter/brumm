package tui

import (
	tea "charm.land/bubbletea/v2"
	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
	"net/url"
	"os/exec"
	"strings"
)

type featureForm struct {
	item   apple.Item
	input  string
	parent string
}
type featureSavedMsg struct{}

func (m *Model) featureKey(msg tea.KeyPressMsg) tea.Cmd {
	f := m.featureForm
	switch msg.String() {
	case "esc":
		m.featureForm = nil
	case "backspace":
		r := []rune(f.input)
		if len(r) > 0 {
			f.input = string(r[:len(r)-1])
		}
	case "ctrl+u":
		f.input = ""
	case "enter":
		text := strings.TrimSpace(f.input)
		if text == "" {
			return nil
		}
		m.featureForm = nil
		if strings.HasPrefix(f.item.Route, "action:lookup:") {
			it := f.item
			it.Route = "app:lookup:" + strings.TrimPrefix(it.Route, "action:lookup:") + ":" + text
			it.ID = it.Route
			it.Name = "Lookup: " + text
			return m.push(&view{title: it.Name, key: it.Key(), item: &it}, "")
		}
		kind := strings.TrimPrefix(f.item.Route, "action:create:")
		client := m.client
		return func() tea.Msg {
			_, err := client.Do(ipc.Request{Cmd: ipc.CmdCreate, List: kind, Query: text, Start: f.parent})
			if err != nil {
				return errMsg{err}
			}
			return featureSavedMsg{}
		}
	default:
		f.input += msg.Text
	}
	return nil
}
func (m *Model) featureBox() []string {
	f := m.featureForm
	w := min(54, m.width-2*margin-4)
	lines := []string{fit(" "+f.input+"▏", w-3), fit(" enter save · esc cancel", w-3)}
	return strings.Split(box(f.item.Name, false, "", lines, w, 4), "\n")
}
func (m *Model) inspectSelected() tea.Cmd {
	v := m.cur()
	if v.sel >= len(v.rows) {
		return nil
	}
	r := v.rows[v.sel]
	var details []apple.Detail
	var title, id string
	if r.track != nil {
		details = r.track.Details
		title = r.track.Title
		id = r.track.ID
	}
	if r.item != nil {
		details = r.item.Details
		title = r.item.Name
		id = r.item.ID
	}
	if title == "" {
		return nil
	}
	page := &view{title: title + " · details", key: "details:" + id, loaded: true}
	for _, d := range details {
		page.rows = append(page.rows, row{note: d.Label + ": " + d.Value})
	}
	if len(page.rows) == 0 {
		page.rows = append(page.rows, row{note: "No additional metadata available."})
	}
	if r.track != nil || (r.item != nil && (r.item.Kind == apple.KindAlbum || r.item.Kind == apple.KindArtist || r.item.Kind == apple.KindPlaylist)) {
		it := apple.Item{Kind: apple.KindShelf, ID: id, Name: "Add favorite", Route: "action:favorite:" + func() string {
			if r.item != nil {
				return r.item.Kind
			}
			return "song"
		}()}
		page.rows = append(page.rows, row{item: &it})
	}
	return m.push(page, "")
}

func (m *Model) openMusicVideo(it apple.Item) tea.Cmd {
	if it.URL == "" {
		return m.push(&view{title: it.Name, key: it.Key(), item: &it}, "")
	}
	u, err := url.Parse(it.URL)
	if err != nil || u.Scheme != "https" || u.Hostname() != "music.apple.com" {
		m.setFlash("No Apple Music video link available")
		return nil
	}
	return func() tea.Msg {
		if err := exec.Command("xdg-open", it.URL).Run(); err != nil {
			return errMsg{err}
		}
		return flashMsg("Video opened in Apple Music")
	}
}
