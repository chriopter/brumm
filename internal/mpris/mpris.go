// Package mpris exposes the daemon as an MPRIS media player, which gives
// brumm media keys, playerctl, the Omarchy bar and the lock screen for free.
package mpris

import (
	"crypto/sha256"
	"fmt"
	"math"
	"sync"
	"time"

	"github.com/godbus/dbus/v5"
	"github.com/godbus/dbus/v5/introspect"

	"github.com/chriopter/brumm/internal/engine"
)

const (
	busName    = "org.mpris.MediaPlayer2.brumm"
	objectPath = dbus.ObjectPath("/org/mpris/MediaPlayer2")
	ifaceRoot  = "org.mpris.MediaPlayer2"
	ifacePlay  = "org.mpris.MediaPlayer2.Player"
	ifaceProps = "org.freedesktop.DBus.Properties"
	noTrack    = dbus.ObjectPath("/org/brumm/track/none")
)

// Controller is what MPRIS clients can ask the player to do.
type Controller interface {
	Play()
	Pause()
	Toggle()
	Next()
	Prev()
	Seek(sec float64)
	SetVolume(v float64)
	SetShuffle(on bool)
	SetRepeat(mode int) // 0 off, 1 one, 2 all
	Raise()
}

type Server struct {
	conn *dbus.Conn
	ctl  Controller

	mu     sync.Mutex
	state  engine.State
	at     time.Time // when state.Pos was sampled
	lastID dbus.ObjectPath
}

func Start(ctl Controller) (*Server, error) {
	conn, err := dbus.ConnectSessionBus()
	if err != nil {
		return nil, err
	}
	s := &Server{conn: conn, ctl: ctl, at: time.Now(), lastID: noTrack}

	// ExportMethodTable rather than Export: a method named Seek would trip
	// go vet's io.Seeker signature check.
	root := map[string]any{
		"Raise": func() *dbus.Error { ctl.Raise(); return nil },
		"Quit":  func() *dbus.Error { return nil },
	}
	player := map[string]any{
		"Play":      func() *dbus.Error { ctl.Play(); return nil },
		"Pause":     func() *dbus.Error { ctl.Pause(); return nil },
		"PlayPause": func() *dbus.Error { ctl.Toggle(); return nil },
		"Stop":      func() *dbus.Error { ctl.Pause(); return nil },
		"Next":      func() *dbus.Error { ctl.Next(); return nil },
		"Previous":  func() *dbus.Error { ctl.Prev(); return nil },
		"Seek": func(offsetUS int64) *dbus.Error {
			ctl.Seek(s.position() + float64(offsetUS)/1e6)
			return nil
		},
		"SetPosition": func(id dbus.ObjectPath, posUS int64) *dbus.Error {
			s.mu.Lock()
			current, dur := s.lastID, s.state.Dur
			s.mu.Unlock()
			if id == current && posUS >= 0 && float64(posUS)/1e6 <= dur {
				ctl.Seek(float64(posUS) / 1e6)
			}
			return nil
		},
		"OpenUri": func(string) *dbus.Error { return nil },
	}
	props := map[string]any{
		"Get": func(iface, name string) (dbus.Variant, *dbus.Error) {
			v, ok := s.properties(iface)[name]
			if !ok {
				return dbus.Variant{}, dbus.MakeFailedError(fmt.Errorf("no property %s.%s", iface, name))
			}
			return v, nil
		},
		// Built fresh per call: a shared map mutated in place leaked the
		// previous track's artUrl and raced concurrent GetAll calls in vibez.
		"GetAll": func(iface string) (map[string]dbus.Variant, *dbus.Error) {
			return s.properties(iface), nil
		},
		"Set": func(iface, name string, v dbus.Variant) *dbus.Error {
			if iface != ifacePlay {
				return nil
			}
			switch val := v.Value().(type) {
			case float64:
				if name == "Volume" {
					ctl.SetVolume(math.Max(0, math.Min(1, val)))
				}
			case bool:
				if name == "Shuffle" {
					ctl.SetShuffle(val)
				}
			case string:
				if name == "LoopStatus" {
					ctl.SetRepeat(map[string]int{"None": 0, "Track": 1, "Playlist": 2}[val])
				}
			}
			return nil
		},
	}
	for iface, table := range map[string]map[string]any{ifaceRoot: root, ifacePlay: player, ifaceProps: props} {
		if err := conn.ExportMethodTable(table, objectPath, iface); err != nil {
			conn.Close()
			return nil, err
		}
	}
	if err := conn.Export(introspect.Introspectable(introspection), objectPath, "org.freedesktop.DBus.Introspectable"); err != nil {
		conn.Close()
		return nil, err
	}

	reply, err := conn.RequestName(busName, dbus.NameFlagDoNotQueue)
	if err != nil || reply != dbus.RequestNameReplyPrimaryOwner {
		conn.Close()
		return nil, fmt.Errorf("mpris: %s is taken (is another brumm running?)", busName)
	}
	return s, nil
}

// position extrapolates the playhead from the last sample.
func (s *Server) position() float64 {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.positionLocked()
}

func (s *Server) positionLocked() float64 {
	p := s.state.Pos
	if s.state.Playing {
		p += time.Since(s.at).Seconds()
	}
	if s.state.Dur > 0 {
		p = math.Min(p, s.state.Dur)
	}
	return p
}

func trackID(st engine.State) dbus.ObjectPath {
	if st.Title == "" {
		return noTrack
	}
	// Object paths allow only [A-Za-z0-9_]; hash whatever identifies the track.
	sum := sha256.Sum256([]byte(fmt.Sprintf("%s\x00%s\x00%s\x00%d", st.ID, st.Title, st.Artist, st.Index)))
	return dbus.ObjectPath(fmt.Sprintf("/org/brumm/track/t%x", sum[:8]))
}

func (s *Server) properties(iface string) map[string]dbus.Variant {
	s.mu.Lock()
	defer s.mu.Unlock()
	st := s.state
	switch iface {
	case ifaceRoot:
		return map[string]dbus.Variant{
			"CanQuit":             dbus.MakeVariant(false),
			"CanRaise":            dbus.MakeVariant(true),
			"HasTrackList":        dbus.MakeVariant(false),
			"Identity":            dbus.MakeVariant("brumm"),
			"DesktopEntry":        dbus.MakeVariant("brumm"),
			"SupportedUriSchemes": dbus.MakeVariant([]string{}),
			"SupportedMimeTypes":  dbus.MakeVariant([]string{}),
		}
	case ifacePlay:
		has := st.Title != ""
		return map[string]dbus.Variant{
			"PlaybackStatus": dbus.MakeVariant(status(st)),
			"LoopStatus":     dbus.MakeVariant(loopStatus(st.Repeat)),
			"Rate":           dbus.MakeVariant(1.0),
			"Shuffle":        dbus.MakeVariant(st.Shuffle),
			"Metadata":       dbus.MakeVariant(metadata(st, s.lastID)),
			"Volume":         dbus.MakeVariant(st.Volume),
			"Position":       dbus.MakeVariant(int64(s.positionLocked() * 1e6)),
			"MinimumRate":    dbus.MakeVariant(1.0),
			"MaximumRate":    dbus.MakeVariant(1.0),
			"CanGoNext":      dbus.MakeVariant(has && st.Index < st.Length-1),
			"CanGoPrevious":  dbus.MakeVariant(has),
			"CanPlay":        dbus.MakeVariant(st.Ready),
			"CanPause":       dbus.MakeVariant(has),
			"CanSeek":        dbus.MakeVariant(has),
			"CanControl":     dbus.MakeVariant(true),
		}
	}
	return map[string]dbus.Variant{}
}

func loopStatus(repeat int) string {
	return [...]string{"None", "Track", "Playlist"}[max(0, min(repeat, 2))]
}

func status(st engine.State) string {
	switch {
	case st.Playing:
		return "Playing"
	case st.Title != "":
		return "Paused"
	}
	return "Stopped"
}

// metadata always carries every key, empty ones included, so no client
// keeps a stale value from the previous track.
func metadata(st engine.State, id dbus.ObjectPath) map[string]dbus.Variant {
	artists := []string{}
	if st.Artist != "" {
		artists = []string{st.Artist}
	}
	return map[string]dbus.Variant{
		"mpris:trackid": dbus.MakeVariant(id),
		"mpris:length":  dbus.MakeVariant(int64(st.Dur * 1e6)),
		"mpris:artUrl":  dbus.MakeVariant(st.Artwork),
		"xesam:title":   dbus.MakeVariant(st.Title),
		"xesam:artist":  dbus.MakeVariant(artists),
		"xesam:album":   dbus.MakeVariant(st.Album),
	}
}

// Update publishes a new player state, signalling only what changed. A jump
// of more than two seconds against the extrapolated playhead is a seek.
func (s *Server) Update(st engine.State) {
	s.mu.Lock()
	prev, expected := s.state, s.positionLocked()
	id := trackID(st)
	trackChanged := id != s.lastID
	s.state, s.at, s.lastID = st, time.Now(), id
	s.mu.Unlock()

	changed := map[string]dbus.Variant{}
	if trackChanged || prev.Dur != st.Dur || prev.Artwork != st.Artwork {
		changed["Metadata"] = dbus.MakeVariant(metadata(st, id))
		changed["CanGoNext"] = dbus.MakeVariant(st.Title != "" && st.Index < st.Length-1)
		changed["CanGoPrevious"] = dbus.MakeVariant(st.Title != "")
		changed["CanPause"] = dbus.MakeVariant(st.Title != "")
		changed["CanSeek"] = dbus.MakeVariant(st.Title != "")
	}
	if status(prev) != status(st) {
		changed["PlaybackStatus"] = dbus.MakeVariant(status(st))
	}
	if prev.Volume != st.Volume {
		changed["Volume"] = dbus.MakeVariant(st.Volume)
	}
	if prev.Shuffle != st.Shuffle {
		changed["Shuffle"] = dbus.MakeVariant(st.Shuffle)
	}
	if prev.Repeat != st.Repeat {
		changed["LoopStatus"] = dbus.MakeVariant(loopStatus(st.Repeat))
	}
	if prev.Ready != st.Ready {
		changed["CanPlay"] = dbus.MakeVariant(st.Ready)
	}
	if len(changed) > 0 {
		_ = s.conn.Emit(objectPath, ifaceProps+".PropertiesChanged", ifacePlay, changed, []string{})
	}
	if !trackChanged && st.Title != "" && math.Abs(st.Pos-expected) > 2 {
		_ = s.conn.Emit(objectPath, ifacePlay+".Seeked", int64(st.Pos*1e6))
	}
}

func (s *Server) Close() {
	_, _ = s.conn.ReleaseName(busName)
	_ = s.conn.Close()
}

const introspection = `<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/><method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="DesktopEntry" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/><method name="Previous"/><method name="Pause"/>
    <method name="PlayPause"/><method name="Stop"/><method name="Play"/>
    <method name="Seek"><arg name="Offset" type="x" direction="in"/></method>
    <method name="SetPosition"><arg name="TrackId" type="o" direction="in"/><arg name="Position" type="x" direction="in"/></method>
    <method name="OpenUri"><arg name="Uri" type="s" direction="in"/></method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="LoopStatus" type="s" access="readwrite"/>
    <property name="Rate" type="d" access="read"/>
    <property name="Shuffle" type="b" access="readwrite"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="readwrite"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="Get"><arg name="interface" type="s" direction="in"/><arg name="property" type="s" direction="in"/><arg name="value" type="v" direction="out"/></method>
    <method name="GetAll"><arg name="interface" type="s" direction="in"/><arg name="properties" type="a{sv}" direction="out"/></method>
    <method name="Set"><arg name="interface" type="s" direction="in"/><arg name="property" type="s" direction="in"/><arg name="value" type="v" direction="in"/></method>
    <signal name="PropertiesChanged"><arg name="interface" type="s"/><arg name="changed" type="a{sv}"/><arg name="invalidated" type="as"/></signal>
  </interface>
</node>`
