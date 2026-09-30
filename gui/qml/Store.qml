// Store is the player's mind, the terminal player's model (internal/tui)
// in QML: one navigation stack per section, what plays, and every key.
// The window's parts only draw what it holds.
import QtQuick

Item {
    id: s
    visible: false

    readonly property var sectionNames: ["Home", "Playlists", "Albums", "Artists", "Songs", "Search", "Queue", "Radio"]
    readonly property var sectionIcons: ["󰋜", "󰲸", "󰀥", "󰠃", "󰎈", "󰍉", "󰐑", "󰐹"]
    readonly property var sectionLists: ["playlists", "albums", "artists", "songs"]
    readonly property int secHome: 0
    readonly property int secPlaylists: 1
    readonly property int secAlbums: 2
    readonly property int secArtists: 3
    readonly property int secSongs: 4
    readonly property int secSearch: 5
    readonly property int secQueue: 6
    readonly property int secRadio: 7
    readonly property int queueAfter: 500

    // ── what shows ──────────────────────────────────────────────────────
    property int section: 0
    property var stacks: []
    property var view: null   // the view on top of the section's stack
    property var rows: []     // its rows; replaced only when they change
    property int sel: 0       // its selection
    property int rev: 0       // counts changes to the view, for bindings
    property string crumbs: ""

    property bool searching: false // the search box has focus
    property bool filtering: false // typing into the list's filter
    property string query: ""

    // ── what plays ──────────────────────────────────────────────────────
    property var st: ({ status: "starting" })
    property double stateAt: Date.now()
    property var pending: null     // a song started, not yet reported playing
    property double seekTo: 0
    property double seekAt: 0
    property double lastVol: 1
    property bool resumed: false
    property bool previewing: false
    property bool spaceDown: false
    property var upNext: []        // songs after the playing one
    property var recent: []        // songs played before it, the latest last (the coverflow's left side)
    property int ahead: 0
    property string nextFor: ""

    property string artwork: ""     // the cover on the stage (stageArtwork)
    property string artKey: ""      // what it was worked out for
    property var rating: ({})
    property int ratingRev: 0

    // ── popups ──────────────────────────────────────────────────────────
    property bool help: false
    property bool sideFocus: false // the keys move through the sections on the left
    // ── the menu ────────────────────────────────────────────────────────
    // The one menu: what to do that is not on screen already, the shared
    // options as switches, and the ways out. Its rows, top to bottom:
    //   {act: key, icon, label} runs what key does · {option: name, label,
    //   hint} is a switch · {heading: text} · {sep: true}
    property bool addOpen: false   // the + on the stage: to a playlist or the library
    property bool menuOpen: false
    property int menuSel: -1 // the row the keys are on; -1 none (opened by a click)
    readonly property var menuRows: {
        const rows = []
        if (st.status === "logged-out")
            rows.push({ act: "L", icon: "󰍂", label: "Sign In to Apple Music…" }, { sep: true })
        rows.push({ act: "f", icon: "󰊓", label: "Full-Screen Visualizer" },
                  { act: "g", icon: "󰆍", label: "Open in Terminal" },
                  { act: "y", icon: "󰌷", label: "Copy Song Link" },
                  { sep: true }, { heading: "Options" },
                  { option: "cover_colors", label: "Cover Colors", hint: "Use the cover's colors for the app." },
                  { option: "autoplay", label: "Autoplay", hint: "Play on after the last song." },
                  { option: "reduce_motion", label: "Reduce Motion", hint: "No scrolling text, fewer wobbles." },
                  { option: "wall", label: "Album Wall", hint: "Covers around the cover, not a flow." })
        if (daemon.omarchy)
            rows.push({ option: "bar", label: "Show in Top Bar", hint: "The song in Omarchy's top bar." })
        rows.push({ sep: true },
                  { act: "U", icon: "󰚰", label: st.update ? "Update to " + st.update + "…" : "Check for Updates…" },
                  { act: "?", icon: "󰋖", label: "Keyboard Shortcuts" },
                  { sep: true },
                  { act: "Q", icon: "󰅖", label: "Close Window" },
                  { act: "q", icon: "󰐥", label: "Quit brumm" })
        return rows
    }
    function menuPickable(r) { return !!r && (r.act !== undefined || r.option !== undefined) }

    // optionOn says whether a switch in the menu is on.
    // The stage shows the cover in a wall of covers, or in a flow: the
    // window's own choice, kept with its settings.
    property bool wall: sys.setting("wall", "true") !== "false"

    function optionOn(name) {
        const o = daemon.options
        switch (name) {
        case "wall": return wall
        case "cover_colors": return !o.no_cover_colors
        case "autoplay": return !o.no_autoplay
        case "reduce_motion": return !!o.reduce_motion
        case "bar": return o.bar === "on"
        }
        return false
    }
    function setOptionOn(name, on) { if (optionOn(name) !== on) changeOption(name) }

    // openMenu opens it; from the keyboard, on its first switch when asked.
    function openMenu(onOption) {
        menuOpen = true
        help = false
        menuSel = onOption ? menuRows.findIndex(r => r.option !== undefined) : -1
    }
    function closeMenu() { menuOpen = false; menuSel = -1 }

    // menuRow acts on row i: an action runs and closes the menu; a switch
    // flips and the menu stays, so several can be set in one visit.
    function menuRow(i) {
        const r = menuRows[i]
        if (!menuPickable(r)) return
        if (r.option !== undefined) { menuSel = i; return changeOption(r.option) }
        closeMenu()
        dispatch(r.act, { text: r.act, modifiers: 0 })
    }

    function menuKey(k) {
        const rows = menuRows, n = rows.length
        const stepTo = d => {
            let i = menuSel < 0 ? (d > 0 ? -1 : n) : menuSel
            for (let t = 0; t < n; t++) {
                i = (i + d + n) % n
                if (menuPickable(rows[i])) { menuSel = i; return }
            }
        }
        const r = rows[menuSel]
        switch (k) {
        case "up": case "k": case "shift+tab": return stepTo(-1)
        case "down": case "j": case "tab": return stepTo(1)
        case "enter": case "space": case " ": return menuSel >= 0 ? menuRow(menuSel) : stepTo(1)
        case "right": case "l": if (r && r.option !== undefined) setOptionOn(r.option, true); return
        case "left": case "h": if (r && r.option !== undefined) setOptionOn(r.option, false); return
        case "esc": case "o": case "q": return closeMenu() // q closes, as it does every popup; the row quits
        }
        const i = rows.findIndex(x => x.act === k) // a row's own key does what it says
        if (i >= 0) return menuRow(i)
        closeMenu()
        dispatch(k)
    }
    property var pick: null        // add to playlist: {what, name, lists, loading, sel, naming, input}
    property var upd: null         // the update check: {checking, installing, installed, newer, current, latest, err}
    property bool full: false      // the visualizer (f)

    // ── the visualizer ──────────────────────────────────────────────────
    // Its styles, named and ordered as the terminal player's, best first.
    readonly property var vizNames: ["milkdrop", "synth", "stars", "wire", "fireworks", "lava", "scope", "plasma", "fire", "matrix", "ridge", "led"]
    readonly property var vizFiles: ["Milkdrop", "Synth", "Stars", "Wire", "Fireworks", "Lava", "Scope", "Plasma", "Fire", "Matrix", "Ridge", "Led"]
    property int vizStyle: 0       // it opens on the showpiece
    property bool vizQuick: false  // the last change is shown at once, not dissolved (moving through the list)
    property bool vizList: false   // v: the list of styles shows
    property int vizSel: 0
    property int vizFrom: 0        // the style before the list: esc goes back to it

    function vizShow(i, quick) {
        vizQuick = !!quick
        vizStyle = (i + vizNames.length) % vizNames.length
    }
    function vizStep(d) { vizShow(vizStyle + d) }
    function vizPick(i) { vizList = false; vizShow(i) }

    // vizKey answers a key while the visualizer shows, as the terminal
    // player's fullKey; false when it is not the visualizer's.
    function vizKey(k) {
        const n = vizNames.length
        const digit = k.length === 1 && k >= "0" && k <= "9" ? (k === "0" ? 9 : parseInt(k) - 1) : -1
        if (vizList) {
            switch (k) {
            case "up": case "k": case "shift+tab": vizSel = (vizSel + n - 1) % n; vizShow(vizSel, true); break
            case "down": case "j": case "tab": vizSel = (vizSel + 1) % n; vizShow(vizSel, true); break
            case "enter": case "space": case "l": case "v": vizList = false; break
            case "esc": case "q": case "h": vizShow(vizFrom, true); vizList = false; break
            case "a": vizCycle(); break
            default: if (digit >= 0 && digit < n) vizPick(digit)
            }
            return true
        }
        switch (k) {
        case "esc": case "f": case "q": full = false; return true
        case "tab": case "right": case "down": case "j": case "l": vizStep(1); return true
        case "shift+tab": case "left": case "up": case "k": case "h": vizStep(-1); return true
        case "v": vizList = true; vizSel = vizStyle; vizFrom = vizStyle; return true
        case "a": vizCycle(); return true
        }
        if (digit >= 0 && digit < n) { vizShow(digit); return true }
        return false
    }
    function vizCycle() {
        const stays = !daemon.options.no_viz_cycle
        daemon.setOption("no_viz_cycle", stays)
        setFlash(stays ? "style stays" : "styles change every minute")
    }

    property string flash: ""
    property double split: 0.4

    signal selMoved()              // the list scrolls the selection into view

    Timer { id: flashTimer; interval: 4000; onTriggered: s.flash = "" }
    Timer { id: holdTimer; interval: 280; onTriggered: s.hold() }
    Timer { // the seek keys stopped: one seek, or at the very end the next song
        id: seekTimer
        interval: 250
        onTriggered: {
            if (s.st.dur > 0 && s.seekTo >= s.st.dur - 1) s.next()
            else daemon.send({ cmd: "seek", value: s.seekTo })
        }
    }
    Timer { id: typedTimer; interval: 350; onTriggered: if (s.section === s.secSearch) s.runSearch(false) } // not after leaving it
    TextInput { id: clip; visible: false } // reads the clipboard: a paste lands in it

    // clipboard is the text on the clipboard, trimmed.
    function clipboard() {
        clip.text = ""
        clip.paste()
        const t = clip.text.trim()
        clip.text = ""
        return t
    }

    function setFlash(text) { flash = text; flashTimer.restart() }

    Component.onCompleted: {
        const st = []
        for (let i = 0; i < sectionNames.length; i++) st.push([])
        st[secHome] = [newView("Home", "home:")]
        for (let i = secPlaylists; i <= secSongs; i++)
            st[i] = [newView(sectionNames[i], "list:" + sectionLists[i - secPlaylists])]
        st[secSearch] = [newView("Search", "search:")]
        st[secSearch][0].loaded = true
        st[secQueue] = [newView("Queue", "queue:")]
        st[secRadio] = [newView("Radio", "radio:", { kind: "shelf", id: "radio", name: "Radio", catalog: true })]
        stacks = st
        const f = parseFloat(sys.setting("split", 0.4))
        if (f >= 0.08 && f <= 0.7) split = f
        refresh()
        if (daemon.connected) started()
    }

    // started runs once the daemon first answers: what every start asks.
    property bool updateAsked: false
    function started() {
        if (updateAsked) return
        updateAsked = true
        daemon.send({ cmd: "update" }) // look for an update on every start
        askPlace()
    }

    function newView(title, key, item) {
        return { title: title, key: key, item: item || null, rows: [], loaded: false, loading: false,
                 err: "", sel: 0, qpos: 0, filter: "", all: null, want: "" }
    }

    function stack() { return stacks[section] }
    function cur() { const st = stack(); return st[st.length - 1] }

    // refresh tells the window the view changed.
    function refresh() {
        artwork = stageArtwork()
        const v = cur()
        view = v
        if (rows !== v.rows) rows = v.rows
        sel = v.sel
        crumbs = stack().map(x => x.title).join("  /  ")
        rev++
    }

    function setSel(i) {
        const v = cur()
        v.sel = i
        sel = i
        selMoved()
    }

    function itemKey(it) { return (it.catalog ? "cat" : "lib") + ":" + it.kind + ":" + it.id }
    function selectable(r) { return !!(r && (r.track || r.item)) }

    // ── loading ─────────────────────────────────────────────────────────

    function load(v) {
        if (v.loading || st.status === "logged-out") return
        let req
        if (v.item) req = { cmd: "open", item: v.item }
        else if (v.key.startsWith("list:")) req = { cmd: "list", list: v.key.slice(5) }
        else if (v.key.startsWith("search:") && v.key !== "search:") req = { cmd: "search", query: v.key.slice(7) }
        else if (v.key === "queue:") req = { cmd: "queue", value: 500 }
        else if (v.key === "home:") req = { cmd: "home" }
        else return
        v.loading = true
        if (v === cur()) rev++
        daemon.request(req, (err, reply) => {
            v.loading = false
            if (err) {
                v.err = err
                if (v === cur()) refresh()
                return
            }
            fill(v, reply)
            if (v.want) { selectSong(v, v.want); v.want = "" }
            fetchRatings(v)
            warm(v)
            if (v === cur()) refresh()
            if (!resumed) maybeResume()
        })
    }

    // fill turns a reply into rows: shelves become a heading and their
    // contents; an album or playlist opens with its facts and note.
    function fill(v, reply) {
        const rows = []
        const it = v.item
        if (it && (it.kind === "album" || it.kind === "playlist")) {
            if (it.info) rows.push({ note: it.info })
            if (it.note) rows.push({ note: it.note })
        }
        for (const sh of (reply.shelves || [])) {
            rows.push({ head: sh.title })
            for (const t of (sh.tracks || [])) rows.push({ track: t })
            for (const i of (sh.items || [])) rows.push({ item: i })
        }
        for (const i of (reply.items || [])) rows.push({ item: i })
        for (const t of (reply.tracks || [])) rows.push({ track: t })
        const first = !v.loaded
        v.rows = rows
        v.loaded = true
        v.err = ""
        if (v.all !== null) { v.all = rows; applyFilter(v) }
        v.qpos = Math.max(0, reply.pos || 0)
        if (it && it.kind === "album" && v.title === "Album" && reply.tracks && reply.tracks.length)
            v.title = reply.tracks[0].album
        if (first && v.key === "queue:") { // the queue opens on the song playing
            const i = rows.findIndex(r => r.track && r.track.id === st.id)
            if (i >= 0) v.sel = i
        }
        v.sel = nearest(rows, Math.min(v.sel, Math.max(0, rows.length - 1)), 1)
    }

    function nearest(rows, i, dir) {
        for (const d of [dir, -dir])
            for (let j = i; j >= 0 && j < rows.length; j += d)
                if (selectable(rows[j])) return j
        return Math.max(0, Math.min(i, rows.length - 1))
    }

    function selectSong(v, id) {
        if (!id) return
        for (let i = 0; i < v.rows.length; i++) {
            const t = v.rows[i].track
            const it = v.rows[i].item
            if ((t && (t.id === id || t.id.endsWith(id))) || (it && it.id === id)) { v.sel = i; return }
        }
    }

    function songsIn(v) {
        const ids = [], at = {}
        v.rows.forEach((r, i) => { if (r.track) { at[i] = ids.length; ids.push(r.track.id) } })
        return { ids: ids, at: at }
    }

    function warm(v) {
        if (v.key === "queue:") return
        const ids = songsIn(v).ids.slice(0, 300)
        if (ids.length) daemon.send({ cmd: "warm", ids: ids })
    }

    function fetchRatings(v) {
        const refs = []
        for (const r of v.rows) {
            if (refs.length >= 1000) break
            if (r.track) refs.push({ kind: "song", id: r.track.id })
            else if (r.item && ["album", "playlist", "station"].includes(r.item.kind)) refs.push({ kind: r.item.kind, id: r.item.id })
        }
        if (!refs.length) return
        daemon.request({ cmd: "ratings", refs: refs }, (err, reply) => {
            if (err || !reply.ratings) return
            for (const id in reply.ratings) rating[id] = reply.ratings[id]
            ratingRev++
        })
    }

    // ── the daemon's reports ────────────────────────────────────────────

    Connections {
        target: daemon
        function onState(state) { s.gotState(state) }
        function onLibrary() {
            for (let i = s.secPlaylists; i <= s.secSongs; i++)
                for (const v of s.stacks[i]) {
                    if (i === s.section) s.load(v)
                    else v.loaded = false
                }
        }
        function onConnectedChanged() {
            if (!daemon.connected) s.st = Object.assign({}, s.st, { status: "starting", message: "connecting to brumm…" })
            else s.started()
        }
    }

    property bool loginTried: false // the sign-in page opened on its own once

    function gotState(state) {
        const wasLoggedOut = st.status === "logged-out"
        if (state.status === "logged-out" && !loginTried) { loginTried = true; signIn() } // nobody has to hunt for the key
        if (state.update && state.update !== st.update) setFlash("brumm " + state.update + " is available · U updates")
        keepShowing(state)
        const pp = pendingPlay
        if (pp) {
            if (state.playing === pp.playing || Date.now() - pp.at > 6000) pendingPlay = null
            else state.playing = pp.playing // not caught up yet
        }
        const songChanged = state.id !== st.id
        if (songChanged) remember()
        if (state.err && state.err !== st.err) setFlash(state.err)
        // The same song playing on: a report a little off the extrapolated
        // position is not followed, so the bar moves on evenly instead of
        // jumping back and forth by fractions of a second.
        if (state.id === st.id && state.playing && st.playing && Math.abs((state.pos || 0) - position(Date.now())) < 0.75) {
            state.pos = position(Date.now())
        }
        st = state
        stateAt = Date.now()
        const key = [state.id, state.preview ? state.preview.artwork : "", state.artwork].join("|")
        if (key !== artKey) { artKey = key; artwork = stageArtwork() }
        if (songChanged && section === secQueue && stack().length === 1) load(cur())
        if (wasLoggedOut || !cur().loaded) load(cur())
        if (!resumed) maybeResume()
        fetchNext(false)
    }

    // position is where the song is now, between reports; right after a
    // seek, where the seek goes.
    function position(now) {
        const since = now - seekAt
        if (since < 1500) {
            let p = seekTo
            if (st.playing && since > 250) p += (since - 250) / 1000
            return Math.min(p, Math.max(st.dur || 0, p))
        }
        let p = st.pos || 0
        if (st.playing) p += (now - stateAt) / 1000
        return st.dur > 0 ? Math.min(p, st.dur) : p
    }

    // ── playing at once ──────────────────────────────────────────────────
    // The player takes its time — a license, the first bytes, Apple's web
    // player — so the window does not wait for it: what a key or click
    // asks for shows at once, and the player's reports keep showing it
    // until they catch up (or a few seconds pass).

    property var pendingPlay: null // {playing, at}: a pause or play asked for

    function toggle() {
        if (!st.id) return daemon.send({ cmd: "toggle" })
        const playing = !st.playing
        pendingPlay = { playing: playing, at: Date.now() }
        st = Object.assign({}, st, { playing: playing, pos: position(Date.now()) })
        stateAt = Date.now()
        daemon.send({ cmd: "toggle" })
    }

    // stayPlaying keeps the button on pause through a change of song: the
    // player reports "not playing" while it loads the next one.
    function stayPlaying() {
        if (st.playing) pendingPlay = { playing: true, at: Date.now() }
    }

    function next() {
        stayPlaying()
        const t = upNext[0]
        if (t && !st.preview) {
            showPlaying(t, st.source, (st.index || 0) + 1)
            upNext = upNext.slice(1) // a second n before the player answers goes one further
        }
        daemon.send({ cmd: "next" })
    }

    function prev() {
        stayPlaying()
        if (st.dur > 0 && position(Date.now()) > 3) { seekTo = 0; seekAt = Date.now() } // as the player does: back to the start first
        daemon.send({ cmd: "prev" })
    }

    // playSong plays one song, as it was played before (the wall's tiles).
    function playSong(t) {
        showPlaying(t, st.source)
        daemon.request({ cmd: "play", ids: [t.id], start: t.id, source: st.source }, err => { if (err) setFlash(err) })
    }

    // jumpTo plays the i-th song after the one playing.
    function jumpTo(i) {
        pendingPlay = { playing: true, at: Date.now() }
        const t = upNext[i], at = (st.index || 0) + 1 + i
        if (t) {
            showPlaying(t, st.source, at)
            upNext = upNext.slice(i + 1)
        }
        daemon.send({ cmd: "jump", value: at })
    }

    // showPlaying puts a song on the stage at once, as if it played.
    // remember puts the song on the stage to the left of the coverflow,
    // as one that played.
    function remember() {
        if (!st.id || st.preview || !st.title) return
        recent = recent.filter(t => t.id !== st.id).concat([{ id: st.id, title: st.title, artist: st.artist, album: st.album, artwork: artwork }]).slice(-8)
    }

    function showPlaying(t, source, index) {
        if (t.id !== st.id) remember()
        pendingPlay = { playing: true, at: Date.now() } // it starts playing: no flash of "paused" while it loads
        pending = { track: t, source: source, index: index, at: Date.now() }
        const copy = Object.assign({}, st)
        keepShowing(copy)
        st = copy
        stateAt = Date.now()
        artwork = stageArtwork()
    }

    function keepShowing(state) {
        const p = pending
        if (!p) return
        if (state.id === p.track.id || (state.err && state.err !== st.err) || Date.now() - p.at > 6000) {
            pending = null
            return
        }
        const t = p.track
        Object.assign(state, { id: t.id, title: t.title, artist: t.artist, album: t.album, dur: t.duration, pos: 0,
                               source: p.source, playing: true, preview: null })
        if (p.index !== undefined) state.index = p.index
        if (t.artwork) state.artwork = t.artwork
    }

    // albumOf finds an album the lists know by its name and artist: its
    // facts and Apple's note, for the back of the cover.
    function albumOf(name, artist) {
        if (!name) return null
        for (const stack of stacks)
            for (const v of stack) {
                if (v.item && v.item.kind === "album" && v.item.name === name) return v.item
                for (const r of v.rows)
                    if (r.item && r.item.kind === "album" && r.item.name === name && (!artist || r.item.artist === artist)) return r.item
            }
        return null
    }

    // stageArtwork is the cover to show: the preview's, else the playing
    // song's as a list has it, else the player's.
    function stageArtwork() {
        if (st.preview && st.preview.artwork) return st.preview.artwork
        if (st.id)
            for (const stack of stacks)
                for (const v of stack)
                    for (const r of v.rows)
                        if (r.track && r.track.id === st.id && r.track.artwork) return r.track.artwork
        return st.artwork || ""
    }

    function fetchNext(force) {
        const key = (st.id || "") + "|" + (st.index || 0) + "|" + (st.length || 0)
        if (!force && key === nextFor) return
        nextFor = key
        if (!st.id) { upNext = []; ahead = 0; return }
        daemon.request({ cmd: "queue", value: 12 }, (err, reply) => {
            if (err) return
            const tracks = reply.tracks || [], pos = reply.pos || 0
            let i = tracks.findIndex(t => t.id === st.id)
            if (i < 0) i = 0
            upNext = tracks.slice(i + 1)
            ahead = Math.max(0, (st.length || 0) - (st.index || 0) - 1)
        })
    }

    // maybeResume lands on what plays the first time the window opens
    // mid-song.
    function maybeResume() {
        if (resumed || placing || !st.source) return
        const root = stacks[st.source.includes(":album:") ? secAlbums : secPlaylists][0]
        if (!root.loaded) { load(root); return }
        resumed = true
        jumpToPlaying()
    }

    // ── the place ───────────────────────────────────────────────────────
    // Where the player is — the section, the views opened in it, the
    // selected row and the search — so that g opens the other one right
    // there. The daemon keeps it without reading it; the terminal player
    // (internal/tui/place.go) writes and reads the same shape:
    //   {section, views: [{key, title, item}], sel: {id, index}, query}

    property bool placing: true // asking for a place the terminal left: no resuming till it answers

    function here() {
        const v = cur(), r = v.rows[v.sel]
        return { section: section, query: query,
                 views: stack().slice(1).filter(x => x.item).map(x => ({ key: x.key, title: x.title, item: x.item })),
                 sel: { id: r ? (r.track ? r.track.id : r.item ? r.item.id : "") : "", index: v.sel } }
    }

    // askPlace takes a place left by the terminal player, off the daemon so
    // a later start opens as ever; with none, the window resumes as ever.
    function askPlace() {
        daemon.request({ cmd: "place" }, (err, reply) => {
            placing = false
            const p = !err && reply ? reply.place : null
            if (!p || typeof p !== "object") return maybeResume()
            daemon.send({ cmd: "place", place: null })
            resumed = true
            goPlace(p)
        })
    }

    function goPlace(p) {
        const sec = Math.max(0, Math.min(p.section | 0, sectionNames.length - 1))
        query = p.query || ""
        const q = query.trim()
        if (q.length >= 2) stacks[secSearch] = [newView("Search", "search:" + q)]
        const st = [stacks[sec][0]]
        for (const pv of (p.views || []))
            if (pv.item && pv.key) st.push(newView(pv.title || pv.item.name || "", pv.key, pv.item))
        stacks[sec] = st
        section = sec
        searching = false
        filtering = false
        const top = st[st.length - 1], sel = p.sel || {}
        top.sel = Math.max(0, sel.index | 0)
        top.want = sel.id || ""
        for (const v of st) {
            if (sec === secQueue) v.loaded = false // always fresh
            if (!v.loaded) load(v)
            else if (v === top) { selectSong(v, v.want); v.want = "" }
        }
        refresh()
        selMoved()
    }

    // ── navigation ──────────────────────────────────────────────────────

    function switchTo(i) {
        section = i
        help = false
        if (i === secQueue) {
            stacks[secQueue] = stacks[secQueue].slice(0, 1)
            cur().loaded = false
        }
        searching = false // the box stays behind; at Search, typing a letter goes into it (typesSearch)
        if (filtering) filtering = false
        if (!cur().loaded) load(cur())
        refresh()
        selMoved()
    }

    function push(v, song) {
        const st = stack()
        const at = st.findIndex(o => o.key === v.key)
        if (at >= 0) {
            stacks[section] = st.slice(0, at + 1)
            v = stacks[section][at]
        } else {
            stacks[section] = st.concat([v])
        }
        if (v.loaded) selectSong(v, song)
        else {
            if (song) v.want = song
            load(v)
        }
        refresh()
        selMoved()
    }

    function back() {
        const st = stack()
        if (st.length > 1) {
            stacks[section] = st.slice(0, -1)
            refresh()
            selMoved()
        }
    }

    function activate() {
        const v = cur()
        const r = v.rows[v.sel]
        if (!selectable(r)) return
        if (r.item) {
            const it = r.item
            if (it.kind === "station") return startStation({ cmd: "station", item: it })
            if (it.kind === "term") { query = it.name; return runSearch(true) }
            return push(newView(it.name, itemKey(it), it), "")
        }
        if (v.key === "queue:") {
            showPlaying(r.track, st.source, v.qpos + v.sel)
            return daemon.send({ cmd: "jump", value: v.qpos + v.sel })
        }
        const so = songsIn(v)
        const i = so.at[v.sel]
        let window = so.ids.slice(i, i + queueAfter)
        if (st.shuffle && so.ids.length > window.length) {
            const rest = so.ids.filter(id => id !== r.track.id)
            for (let j = rest.length - 1; j > 0; j--) {
                const k = Math.floor(Math.random() * (j + 1));
                [rest[j], rest[k]] = [rest[k], rest[j]]
            }
            window = [r.track.id].concat(rest.slice(0, queueAfter - 1))
        }
        showPlaying(r.track, v.key)
        daemon.request({ cmd: "play", ids: window, start: r.track.id, source: v.key }, err => { if (err) setFlash(err) })
    }

    // activateAt plays or opens row i, as a double click does.
    function activateAt(i) { setSel(i); activate() }

    function jumpToPlaying() {
        const src = st.source, id = st.id
        if (!src) return
        const pickSong = v => {
            const i = v.rows.findIndex(r => r.track && r.track.id === id)
            if (i >= 0) v.sel = i
        }
        for (let i = secHome; i <= secSearch; i++) {
            const at = stacks[i].findIndex(v => v.key === src)
            if (at >= 0) {
                section = i
                stacks[i] = stacks[i].slice(0, at + 1)
                pickSong(stacks[i][at])
                refresh()
                selMoved()
                return
            }
        }
        for (let i = secPlaylists; i <= secArtists; i++) {
            const r = stacks[i][0].rows.find(r => r.item && itemKey(r.item) === src)
            if (r) {
                section = i
                const v = newView(r.item.name, src, r.item)
                v.want = id
                stacks[i] = [stacks[i][0], v]
                load(v)
                refresh()
                return
            }
        }
    }

    function selectedTrack() {
        const v = cur()
        const r = v.rows[v.sel]
        if (r && r.track) return r.track.id
        return st.id || ""
    }

    function lookup(cmd, id) {
        id = id || selectedTrack()
        if (!id) return
        daemon.request({ cmd: cmd, start: id }, (err, reply) => {
            if (err || !reply.items || !reply.items.length) return setFlash("nothing found for this song")
            openItem(reply.items[0], id)
        })
    }

    // openItem shows an item found by a lookup in its own section.
    function openItem(it, song) {
        if (it.kind === "artist") section = secArtists
        else if (it.kind === "album") section = secAlbums
        else if (it.kind === "playlist") section = secPlaylists
        searching = false
        push(newView(it.name || (it.kind === "album" ? "Album" : it.kind), itemKey(it), it), song)
    }

    // ── search and filter ───────────────────────────────────────────────

    function filterable() {
        const v = cur()
        return section !== secSearch && v.key !== "queue:" && v.loaded && (v.rows.length + (v.all ? v.all.length : 0)) > 0
    }

    function startFilter() {
        const v = cur()
        if (v.all === null) v.all = v.rows
        filtering = true
        help = false
        rev++
    }

    function applyFilter(v) {
        if (v.all === null) return
        const words = v.filter.toLowerCase().split(/\s+/).filter(w => w)
        if (!words.length) v.rows = v.all
        else v.rows = v.all.filter(r => {
            let text
            if (r.track) text = r.track.title + " " + r.track.artist + " " + r.track.album
            else if (r.item) text = r.item.name + " " + (r.item.artist || "")
            else return false
            text = text.toLowerCase()
            return words.every(w => text.includes(w))
        })
        v.sel = nearest(v.rows, 0, 1)
    }

    function clearFilter(v) {
        if (v.all === null) return
        const keep = v.rows[v.sel]
        v.rows = v.all
        v.all = null
        v.filter = ""
        if (keep) {
            const i = v.rows.indexOf(keep)
            if (i >= 0) v.sel = i
        }
        filtering = false
        refresh()
        selMoved()
    }

    function editText(text, k, ev) {
        if (k === "backspace") return text.slice(0, -1)
        if (k === "ctrl+u") return ""
        if (k === "ctrl+v") return text + clipboard()
        if (k === "ctrl+w") {
            const q = text.replace(/\s+$/, "")
            const i = q.lastIndexOf(" ")
            return i >= 0 ? q.slice(0, i + 1) : ""
        }
        if (ev && ev.text && ev.text.length && ev.text >= " " && !(ev.modifiers & Qt.ControlModifier)) return text + ev.text
        return null
    }

    function filterKey(k, ev) {
        const v = cur()
        switch (k) {
        case "esc":
            clearFilter(v)
            return
        case "enter":
            filtering = false
            if (!v.rows.length && v.filter.trim()) return searchEverywhere(v.filter)
            rev++
            return
        case "tab":
            if (v.filter.trim()) return searchEverywhere(v.filter)
            return
        case "up": case "down": case "pgup": case "pgdown":
            filtering = false
            rev++
            return move(k)
        case "ctrl+c": case "ctrl+q": return Qt.quit()
        }
        const t = editText(v.filter, k, ev)
        if (t === null) return
        v.filter = t
        applyFilter(v)
        refresh()
        selMoved()
    }

    function searchEverywhere(q) {
        filtering = false
        clearFilter(cur())
        query = q
        section = secSearch
        searching = false
        runSearch(true)
    }

    function searchKey(k, ev) {
        switch (k) {
        case "esc":
            searching = false
            rev++
            return
        case "enter":
            searching = false
            rev++
            return runSearch(true)
        case "down": case "up":
            searching = false
            rev++
            return move(k)
        case "right": // into the results, not playing one
            searching = false
            rev++
            return
        case "left": case "tab": case "shift+tab":
            searching = false
            return key(k)
        case "ctrl+c": case "ctrl+q": return Qt.quit()
        case "1": case "2": case "3": case "4": case "5": case "6": case "7": case "8":
            if (query === "") { // nothing typed yet: a digit is a section, as outside the box
                searching = false
                return key(k)
            }
        }
        const t = editText(query, k, ev)
        if (t === null) return
        query = t
        typedTimer.restart()
    }

    function runSearch(final) {
        const q = query.trim()
        section = secSearch
        if (/^(https?:\/\/)?([a-z]+\.)?music\.apple\.com\//i.test(q)) { // a link: open what it names
            if (!final) return
            searching = false
            stacks[secSearch] = stacks[secSearch].slice(0, 1)
            daemon.request({ cmd: "resolve", query: q }, (err, reply) => {
                if (err || !reply.items || !reply.items.length) return setFlash(err || "could not open that link")
                openItem(reply.items[0], reply.ids ? reply.ids[0] : "")
            })
            return
        }
        if (q.length < 2) { refresh(); return }
        if (stacks[secSearch][0].key === "search:" + q && stacks[secSearch].length === 1) { refresh(); return }
        stacks[secSearch] = [newView("Search", "search:" + q)]
        load(cur())
        refresh()
    }

    // ── moving ──────────────────────────────────────────────────────────

    property int pageRows: 20

    function move(k) {
        const v = cur()
        const n = v.rows.length
        if (!n) return
        const prev = v.sel
        let i = prev
        switch (k) {
        case "up": case "k": i--; break
        case "down": case "j": i++; break
        case "pgup": case "ctrl+u": i -= pageRows; break
        case "pgdown": case "ctrl+d": i += pageRows; break
        case "home": i = 0; break
        case "G": case "end": i = n - 1; break
        default: return
        }
        i = Math.max(0, Math.min(i, n - 1))
        let dir = (i < prev || k === "G" || k === "end") ? -1 : 1
        if (k === "home") dir = 1
        i = nearest(v.rows, i, dir)
        if (i === prev && (k === "up" || k === "k") && section === secSearch && stack().length === 1) {
            searching = true
            rev++
            return
        }
        setSel(i)
    }

    // ── playing ─────────────────────────────────────────────────────────

    function seek(sec) {
        if (!(st.dur > 0)) return
        seekTo = Math.max(0, Math.min(sec, st.dur))
        seekAt = Date.now()
        seekTimer.restart()
    }

    function setVolume(v) {
        v = Math.min(1, Math.max(0, v))
        st = Object.assign({}, st, { volume: v })
        setFlash("volume " + Math.round(v * 100) + "%")
        daemon.send({ cmd: "volume", value: v })
    }

    function spacePressed() {
        if (spaceDown) return
        spaceDown = true
        holdTimer.restart()
    }

    function hold() {
        if (!spaceDown) return
        const r = cur().rows[cur().sel]
        if (!r || !r.track) return
        previewing = true
        daemon.send({ cmd: "preview", start: r.track.id, value: 1 })
    }

    function spaceReleased() {
        if (!spaceDown) return
        spaceDown = false
        holdTimer.stop()
        if (previewing) {
            previewing = false
            daemon.send({ cmd: "preview" })
            return
        }
        toggle()
    }

    function togglePreview() {
        if (previewing) {
            previewing = false
            return daemon.send({ cmd: "preview" })
        }
        const r = cur().rows[cur().sel]
        if (!r || !r.track) return
        previewing = true
        daemon.send({ cmd: "preview", start: r.track.id, value: 1 })
    }

    function startStation(req) {
        setFlash("tuning in…")
        daemon.request(req, (err, reply) => {
            if (err) return setFlash(err)
            if (reply.items && reply.items.length) setFlash("▶ " + reply.items[0].name)
        })
    }

    function playStation() {
        const r = cur().rows[cur().sel]
        if (r && r.track) return startStation({ cmd: "station", start: r.track.id })
        if (r && r.item && (r.item.kind === "station" || r.item.kind === "artist")) return startStation({ cmd: "station", item: r.item })
        if (st.id) return startStation({ cmd: "station", start: st.id })
        setFlash("select a song or an artist to start its station")
    }

    // songsOf lists the songs a row stands for, then calls done(ids).
    function songsOf(r, done) {
        if (r.track) return done([r.track.id])
        if (!r.item || r.item.kind === "station" || r.item.kind === "term") return setFlash("nothing to add here")
        daemon.request({ cmd: "open", item: r.item }, (err, reply) => {
            if (err) return setFlash(err)
            const ids = (reply.tracks || []).map(t => t.id)
            for (const sh of (reply.shelves || [])) for (const t of (sh.tracks || [])) ids.push(t.id)
            done(ids)
        })
    }

    function enqueue(next) {
        const r = cur().rows[cur().sel]
        if (!r) return
        songsOf(r, ids => {
            if (!ids.length) return
            daemon.request({ cmd: "enqueue", ids: ids, value: next ? 1 : 0 }, err => {
                setFlash(err ? err : next ? "plays next" : "added to the queue")
                fetchNext(true)
            })
        })
    }

    // ── the song playing ────────────────────────────────────────────────
    // What the stage's buttons do: the same as the keys, but always for
    // the song playing, whatever the list has selected.
    function playingRow() { return st.id && !st.preview ? { track: { id: st.id, title: st.title, artist: st.artist, album: st.album } } : null }
    function ratePlaying(value) {
        if (!st.id || st.preview) return
        if (rating[st.id] === value) value = 0
        if (value === 0) delete rating[st.id]
        else rating[st.id] = value
        ratingRev++
        setFlash(value === 1 ? "♥ " + st.title : value === -1 ? "disliked " + st.title : st.title + ": no rating")
        daemon.send({ cmd: "rate", refs: [{ kind: "song", id: st.id }], value: value })
    }
    function pickPlaying() {
        const r = playingRow()
        if (!r) return
        pick = { what: r, name: st.title, lists: [], loading: true, sel: 0, naming: false, input: "" }
        help = false
        daemon.request({ cmd: "list", list: "playlists" }, (err, reply) => {
            if (!pick) return
            pick = Object.assign({}, pick, { loading: false, lists: (reply && reply.items || []).filter(i => i.editable) })
        })
    }
    function libraryPlaying() {
        if (!st.id || st.preview) return
        if (st.id.startsWith("i.")) return setFlash(st.title + " is already in your library")
        daemon.request({ cmd: "add", list: "songs", ids: [st.id] }, err => setFlash(err ? err : "added " + st.title + " to your library"))
    }
    function radioPlaying() { if (st.id) startStation({ cmd: "station", start: st.id }) }
    function linkPlaying() {
        if (!st.id) return
        daemon.request({ cmd: "link", start: st.id }, (err, reply) => {
            if (err) return setFlash(err)
            sys.copy(reply.link)
            setFlash("link copied")
        })
    }

    function rateable() {
        const r = cur().rows[cur().sel]
        if (r && r.track) return { ref: { kind: "song", id: r.track.id }, name: r.track.title }
        if (r && r.item && ["album", "playlist", "station"].includes(r.item.kind)) return { ref: { kind: r.item.kind, id: r.item.id }, name: r.item.name }
        if (st.id && !st.preview) return { ref: { kind: "song", id: st.id }, name: st.title }
        return null
    }

    function rate(value) {
        const it = rateable()
        if (!it) return
        if (rating[it.ref.id] === value) value = 0
        if (value === 0) delete rating[it.ref.id]
        else rating[it.ref.id] = value
        ratingRev++
        setFlash(value === 1 ? "♥ " + it.name : value === -1 ? "disliked " + it.name : it.name + ": no rating")
        daemon.send({ cmd: "rate", refs: [it.ref], value: value })
    }

    function addToLibrary() {
        const r = cur().rows[cur().sel]
        let kind, id, name
        if (r && r.track) { kind = "songs"; id = r.track.id; name = r.track.title }
        else if (r && r.item) {
            if (r.item.kind === "artist") return setFlash("artists cannot be added; add one of their albums")
            if (!r.item.catalog) return setFlash(r.item.name + " is already in your library")
            kind = r.item.kind + "s"; id = r.item.id; name = r.item.name
        } else if (st.id) { kind = "songs"; id = st.id; name = st.title }
        else return
        if (id.startsWith("i.")) return setFlash(name + " is already in your library")
        daemon.request({ cmd: "add", list: kind, ids: [id] }, err => setFlash(err ? err : "added " + name + " to your library"))
    }

    function copyLink() {
        const id = selectedTrack()
        if (!id) return
        daemon.request({ cmd: "link", start: id }, (err, reply) => {
            if (err) return setFlash(err)
            sys.copy(reply.link)
            setFlash("link copied")
        })
    }

    // ── add to playlist ─────────────────────────────────────────────────

    function openPicker() {
        const r = cur().rows[cur().sel]
        let what, name
        if (r && r.track) { what = r; name = r.track.title }
        else if (r && r.item && (r.item.kind === "album" || r.item.kind === "playlist")) { what = r; name = r.item.name }
        else if (st.id) { what = { track: { id: st.id, title: st.title } }; name = st.title }
        else return setFlash("select a song, album or playlist to add")
        pick = { what: what, name: name, lists: [], loading: true, sel: 0, naming: false, input: "" }
        help = false
        daemon.request({ cmd: "list", list: "playlists" }, (err, reply) => {
            if (!pick) return
            pick = Object.assign({}, pick, { loading: false, lists: (reply && reply.items || []).filter(i => i.editable) })
        })
    }

    function pickerKey(k, ev) {
        const p = pick
        if (p.naming) {
            if (k === "esc") pick = Object.assign({}, p, { naming: false })
            else if (k === "enter") { if (p.input.trim()) addToPlaylist("", p.input.trim()) }
            else {
                const t = editText(p.input, k, ev)
                if (t !== null) pick = Object.assign({}, p, { input: t })
            }
            return
        }
        const n = p.lists.length + 1
        switch (k) {
        case "up": case "k": pick = Object.assign({}, p, { sel: (p.sel + n - 1) % n }); break
        case "down": case "j": pick = Object.assign({}, p, { sel: (p.sel + 1) % n }); break
        case "enter": case "space": case "l": pickRow(p.sel); break
        case "esc": case "q": case "P": case "h": pick = null; break
        }
    }

    function pickRow(i) {
        const p = pick
        if (i === 0) { pick = Object.assign({}, p, { naming: true, input: p.name, sel: 0 }); return }
        const l = p.lists[i - 1]
        if (l) addToPlaylist(l.id, l.name)
    }

    function addToPlaylist(id, name) {
        const what = pick.what
        pick = null
        setFlash("adding to " + name + "…")
        songsOf(what, ids => {
            const req = { cmd: "playlist", start: id, ids: ids }
            if (!id) req.query = name
            daemon.request(req, err => setFlash(err ? err : !id ? "made the playlist " + name : "added " + ids.length + " songs to " + name))
        })
    }

    // ── update ──────────────────────────────────────────────────────────

    function checkUpdate() {
        if (upd && upd.newer && !upd.installing) return installUpdate()
        upd = { checking: true }
        pick = null; help = false
        daemon.request({ cmd: "update", value: 2 }, (err, reply) => {
            if (!upd) return
            upd = { current: reply ? reply.version : "", latest: reply ? reply.link : "", newer: !!(reply && reply.pos === 1), err: err || "" }
        })
    }

    function installUpdate() {
        upd = Object.assign({}, upd, { installing: true })
        daemon.request({ cmd: "update", value: 1 }, err => {
            if (err) { upd = null; return setFlash("update: " + err) }
            upd = Object.assign({}, upd || {}, { installing: false, installed: true })
        })
    }

    function updateKey(k) {
        const u = upd
        if (u.installing || u.checking) return
        if (u.installed && (k === "enter" || k === "U")) {
            upd = null
            setFlash("restarting…")
            return daemon.send({ cmd: "update", value: 3 })
        }
        if (u.installed) {
            if (["esc", "q", "space"].includes(k)) { upd = null; setFlash("brumm restarts into it at the next pause or song change") }
            return
        }
        if ((k === "enter" || k === "U") && u.newer) return installUpdate()
        if (["enter", "esc", "q", "U", "space"].includes(k)) upd = null
    }

    // ── options ─────────────────────────────────────────────────────────

    function changeOption(name) {
        const o = daemon.options
        switch (name) {
        case "cover_colors": daemon.setOption("no_cover_colors", !o.no_cover_colors); break
        case "autoplay": daemon.setOption("no_autoplay", !o.no_autoplay); break // the daemon tells the player
        case "reduce_motion": daemon.setOption("reduce_motion", !o.reduce_motion); break
        case "wall": wall = !wall; sys.setSetting("wall", wall ? "true" : "false"); break
        case "bar": setBar(o.bar !== "on"); break
        }
    }

    // toTerminal is g: brumm goes over to the terminal player, which is
    // what it opens from now on; g there comes back. The music plays on.
    function toTerminal() {
        setFlash("opening the terminal player…")
        // One after the other: the terminal player must find this place.
        daemon.request({ cmd: "place", place: here() }, () => {
            daemon.setOption("start", "tui", err => {
                if (err) return setFlash(err)
                daemon.request({ cmd: "show", query: "tui" }, err => err ? setFlash(err) : Qt.quit())
            })
        })
    }

    function signIn() {
        setFlash("opening the Apple Music sign-in in your browser…")
        daemon.send({ cmd: "login" })
    }

    // The first start asks once about the bar widget, where omarchy can
    // switch it and it is off.
    readonly property bool barAsk: daemon.omarchy && st.status === "ready" && daemon.options.bar !== "on" && !daemon.options.bar_offered
        && !upd && !pick && !menuOpen && !help && !full
    function barAnswer(yes) {
        daemon.setOption("bar_offered", true)
        if (yes) setBar(true)
    }
    function setBar(on) {
        daemon.setOption("bar", on ? "on" : "off", err => setFlash(err ? "bar widget: " + err : "bar widget " + (on ? "on" : "off")))
    }

    // ── keys ────────────────────────────────────────────────────────────

    // dispatch takes a key the way the terminal player names it.
    function dispatch(k, ev) {
        if (addOpen) { addOpen = false; if (k === "esc") return }
        if (menuOpen) return menuKey(k)
        if (upd) return updateKey(k)
        if (barAsk) {
            if (k === "enter" || k === "y") barAnswer(true)
            else if (k === "esc" || k === "n" || k === "q") barAnswer(false)
            return
        }
        if (pick) return pickerKey(k, ev)
        if (searching) return searchKey(k, ev)
        if (filtering) return filterKey(k, ev)
        if (k === "?" && !full) { help = !help; return } // the visualizer carries its own keys
        if (full && vizKey(k)) return
        if (sideFocus) {
            const n = sectionNames.length
            switch (k) {
            case "up": case "k": case "shift+tab": return switchTo((section + n - 1) % n)
            case "down": case "j": case "tab": return switchTo((section + 1) % n)
            case "right": case "l": case "enter": case "esc": case "left": case "h": sideFocus = false; return
            }
            sideFocus = false // any other key goes to the list as ever
        }
        key(k)
    }

    // typesSearch is whether k, pressed on the Search section's results with
    // the box not focused, starts typing into it: any character but a digit
    // (the sections) and the few keys that mean something everywhere.
    function typesSearch(k) {
        return section === secSearch && stack().length === 1 && !full && k.length === 1 && k > " "
            && !(k >= "0" && k <= "9") && !"/?ofgqQ".includes(k)
    }

    function key(k) {
        if (typesSearch(k)) {
            searching = true
            help = false
            query += k
            typedTimer.restart()
            return
        }
        switch (k) {
        case "q":
            daemon.send({ cmd: "quit" }) // done listening: the music stops, and brumm with it
            Qt.quit()
            return
        case "Q": case "ctrl+c": case "ctrl+q":
            Qt.quit() // close the window; the music plays on
            return
        case "L": return signIn()
        case "1": case "2": case "3": case "4": case "5": case "6": case "7": case "8":
            return switchTo(parseInt(k) - 1)
        case "tab": return switchTo((section + 1) % sectionNames.length)
        case "shift+tab": return switchTo((section + sectionNames.length - 1) % sectionNames.length)
        case "left": sideFocus = true; return // over to the sections
        case "right": return activate()
        case "/":
            if (filterable()) return startFilter()
            section = secSearch
            searching = true
            help = false
            refresh()
            return
        case "f": // it opens on the showpiece
            if (!full) vizShow(0, true)
            full = !full; help = false; vizList = false
            return
        case "n": return next()
        case "p": case "b": return prev()
        case "shift+left": return seek(position(Date.now()) - 10)
        case "shift+right": return seek(position(Date.now()) + 10)
        case "+": case "=": return setVolume((st.volume || 0) + 0.05)
        case "-": return setVolume((st.volume || 0) - 0.05)
        case "m":
            if (st.volume > 0) { lastVol = st.volume; return setVolume(0) }
            return setVolume(lastVol)
        case "s": {
            const on = !st.shuffle
            st = Object.assign({}, st, { shuffle: on })
            setFlash(on ? "shuffle on" : "shuffle off")
            return daemon.send({ cmd: "shuffle", value: on ? 1 : 0 })
        }
        case "r": {
            const next = [2, 0, 1][Math.max(0, Math.min(st.repeat || 0, 2))]
            st = Object.assign({}, st, { repeat: next })
            setFlash(["repeat off", "repeat one", "repeat all"][next])
            return daemon.send({ cmd: "repeat", value: next })
        }
        case "[": case "]":
            split = Math.min(0.7, Math.max(0.08, split + (k === "[" ? -0.05 : 0.05)))
            sys.setSetting("split", split)
            return
        case "c": return jumpToPlaying()
        case "o": return openMenu(true) // the options live in the menu
        case "O": return togglePreview()
        case "i": return addToLibrary()
        case "a": return lookup("album")
        case "A": return lookup("artist")
        case "z": return enqueue(false)
        case "Z": return enqueue(true)
        case "*": return rate(1)
        case "d": return rate(-1)
        case "R": return playStation()
        case "P": return openPicker()
        case "y": return copyLink()
        case "U": return checkUpdate()
        case "g": return toTerminal()
        case "ctrl+v": { // a pasted music.apple.com link opens; words search
            const t = clipboard()
            if (!t) return
            query = t
            return runSearch(true)
        }
        case "esc": case "h": case "backspace":
            if (help) { help = false; return }
            if (cur().all !== null && k === "esc") return clearFilter(cur())
            if (previewing) return togglePreview()
            return back()
        case "enter": case "l": {
            const v = cur()
            if (section === secSearch && stack().length === 1 && !selectable(v.rows[v.sel])) {
                searching = true // no results to open: into the box
                rev++
                return
            }
            return activate()
        }
        default: return move(k)
        }
    }
}
