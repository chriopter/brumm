// The window: the library on a pane of glass to the left, what plays on
// its own stage to the right, all over a band of light that drifts while
// music plays — a player in the spirit of the mid-2000s Sony gear, in
// the colors of the Omarchy theme and of the cover.
import QtQuick
import QtQuick.Window
import QtQuick.Effects

Window {
    id: win
    width: testSize.width || 1280
    height: testSize.height || 820
    minimumWidth: 820
    minimumHeight: 560
    visible: true
    title: store.st.title ? store.st.title + " · " + store.st.artist + " — brumm" : "brumm"
    color: ui.deep

    // The renderer, asked of the window: a QtObject has none to tell.
    readonly property int api: GraphicsInfo.api

    // ── the look ────────────────────────────────────────────────────────
    QtObject {
        id: ui
        readonly property var c: theme.colors
        readonly property color bg: c.background
        readonly property color deep: c.darker_background || Qt.darker(bg, 1.35)
        readonly property color fg: c.foreground
        readonly property color bright: c.bright_foreground || c.foreground
        // Dimmed text is the bright one, fainter: it reads on any light.
        readonly property color dim: Qt.alpha(bright, 0.55)
        readonly property color accent: c.accent
        readonly property color heart: c.red || "#f7768e"
        // Glass: the foreground laid thinly over whatever is behind.
        readonly property color glass: Qt.alpha(bright, theme.dark ? 0.035 : 0.45)
        readonly property color glassHi: Qt.alpha(bright, theme.dark ? 0.07 : 0.7)
        readonly property color edge: Qt.alpha(bright, theme.dark ? 0.09 : 0.25)
        readonly property color shade: Qt.alpha(theme.dark ? "black" : deep, theme.dark ? 0.35 : 0.12)
        // What plays takes the cover's color, unless the options say no.
        property color coverAccent: "transparent"
        readonly property color here: !daemon.options.no_cover_colors && coverAccent.a > 0 ? coverAccent : accent
        readonly property bool calm: !!daemon.options.reduce_motion
        readonly property bool awake: Qt.application.state === Qt.ApplicationActive
        // Shaders and effects need the GPU; the software renderer gets a
        // plain gradient and no glows, and still every cover and word.
        readonly property bool effects: win.api !== GraphicsInfo.Software
        readonly property string sans: sansFont
        readonly property string mono: monoFont
        readonly property real scale: theme.textScale
        function px(n) { return Math.round(n * scale) }
        function tint(base, amount) { return Qt.tint(base, Qt.alpha(here, amount)) }
        // Words over the light stand on a faint shadow, a pixel down.
        readonly property int lift: theme.dark && effects ? Text.Raised : Text.Normal
        readonly property color liftColor: Qt.alpha("black", 0.35)
        // One clock for all that moves while music plays in the window
        // in front — the backdrop's drift, the equalizers, the progress
        // bar — so they move in the same frames, not each in its own: 20
        // ticks a second (8 without effects: only the equalizers). It
        // stands while paused, in the background, minimized, with motion
        // reduced, and under the visualizer.
        property int clock: 0 // milliseconds it has run
        readonly property bool pulsing: pulse.running
        readonly property int radius: px(16)
        readonly property int coverRadius: px(6) // every cover's corners, big or small
        readonly property int gap: px(24)
        Behavior on coverAccent { enabled: !ui.calm; ColorAnimation { duration: 700; easing.type: Easing.InOutQuad } }
    }

    Store { id: store }

    Timer {
        id: pulse
        interval: ui.effects ? 50 : 125
        repeat: true
        running: !!store.st.playing && ui.awake && !ui.calm && win.visibility !== Window.Minimized && !(store.full && full.opacity === 1)
        onTriggered: ui.clock += interval
    }

    // The cover's accent, picked once per cover.
    property string accentFor: ""
    readonly property string artwork: store.artwork
    onArtworkChanged: pickAccent()
    Connections {
        target: theme
        function onChanged() { win.accentFor = ""; win.pickAccent() }
    }
    function pickAccent() {
        if (!artwork) { ui.coverAccent = "transparent"; return }
        if (artwork === accentFor) return
        accentFor = artwork
        covers.accent(artwork, ui.bg)
    }
    Connections {
        target: covers
        function onAccentReady(url, color) {
            if (url === win.accentFor) ui.coverAccent = color.valid ? color : "transparent"
        }
    }

    // ── keys ────────────────────────────────────────────────────────────
    // Named the way the terminal player names them: both take the same.
    function keyName(e) {
        const shift = e.modifiers & Qt.ShiftModifier, ctrl = e.modifiers & Qt.ControlModifier
        switch (e.key) {
        case Qt.Key_Up: return "up"
        case Qt.Key_Down: return "down"
        case Qt.Key_Left: return shift ? "shift+left" : "left"
        case Qt.Key_Right: return shift ? "shift+right" : "right"
        case Qt.Key_Tab: return "tab"
        case Qt.Key_Backtab: return "shift+tab"
        case Qt.Key_Return: case Qt.Key_Enter: return "enter"
        case Qt.Key_Escape: return "esc"
        case Qt.Key_Backspace: return "backspace"
        case Qt.Key_PageUp: return "pgup"
        case Qt.Key_PageDown: return "pgdown"
        case Qt.Key_Home: return "home"
        case Qt.Key_End: return "end"
        case Qt.Key_Space: return "space"
        }
        if (ctrl && e.key >= Qt.Key_A && e.key <= Qt.Key_Z)
            return "ctrl+" + String.fromCharCode(e.key).toLowerCase()
        return e.text
    }

    Timer { // tests: keys to press once the window is up
        running: testKeys !== ""
        interval: 700
        onTriggered: {
            for (const k of testKeys.split(" ")) {
                if (k === "MENU") store.menuOpen = true // the menu opens by a click
                else store.dispatch(k, { text: k.length === 1 ? k : "", modifiers: 0 })
            }
        }
    }

    Backdrop { anchors.fill: parent; visible: ui.effects && !(store.full && full.opacity === 1) }
    Rectangle {
        anchors.fill: parent
        visible: !ui.effects
        gradient: Gradient {
            GradientStop { position: 0; color: ui.tint(ui.deep, 0.1) }
            GradientStop { position: 1; color: ui.tint(ui.deep, 0.22) }
        }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: e => {
            e.accepted = true
            const k = win.keyName(e)
            if (!k) return
            const typing = store.searching || store.filtering || (store.pick && store.pick.naming)
            if (k === "space" && !typing && !store.upd && !store.pick && !store.menuOpen && !store.barAsk) {
                if (!e.isAutoRepeat) store.spacePressed()
                return
            }
            store.dispatch(k, e)
        }
        Keys.onReleased: e => {
            if (e.key === Qt.Key_Space && !e.isAutoRepeat) {
                e.accepted = true
                store.spaceReleased()
            }
        }
    }

    // ── the layout ──────────────────────────────────────────────────────
    readonly property bool popup: store.help || !!store.pick || !!store.upd || store.barAsk

    Item {
        id: body
        anchors { fill: parent; margins: ui.gap; bottomMargin: ui.px(12) }
        // Under a popup, blurred: drawn again only while one shows.
        layer.enabled: win.popup && ui.effects
        layer.effect: MultiEffect { blurEnabled: true; blur: 0.8; blurMax: 40; brightness: -0.12; saturation: -0.2 }
        opacity: store.full ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }

        Sidebar {
            id: sidebar
            z: 2 // its menu hangs over the rest
            anchors { left: parent.left; top: parent.top; bottom: footer.top; leftMargin: -ui.px(6) }
        }

        Header {
            id: header
            anchors { left: sidebar.right; leftMargin: ui.gap; right: parent.right; top: parent.top }
        }

        MouseArea { // a click beside an open menu closes it
            anchors.fill: parent
            z: 1
            visible: store.menuOpen || store.addOpen
            onClicked: { store.menuOpen = false; store.menuSel = -1; store.addOpen = false }
        }

        Browser {
            id: browser
            anchors { left: sidebar.right; leftMargin: ui.gap * 0.75; top: header.bottom; topMargin: ui.px(10); bottom: footer.top; bottomMargin: ui.px(4) }
            width: Math.round(Math.max(ui.px(160), Math.min(parent.width - sidebar.width - ui.px(420), parent.width * store.split)))
        }

        // The line between the list and the stage: invisible, but pointed
        // at it offers to move, and a drag sets the list's width.
        MouseArea {
            id: divider
            x: browser.x + browser.width + ui.gap * 0.4
            width: ui.gap * 0.8
            anchors { top: browser.top; bottom: browser.bottom }
            cursorShape: Qt.SplitHCursor
            preventStealing: true
            // The width follows once a frame, however fast the mouse reports.
            property real want: -1
            onPositionChanged: m => {
                if (!pressed) return
                const at = mapToItem(body, m.x, 0).x - browser.x
                if (want < 0) Qt.callLater(() => { store.split = want; want = -1 })
                want = Math.min(0.7, Math.max(0.08, at / body.width))
            }
            onReleased: sys.setSetting("split", store.split)
        }

        Stage {
            anchors { left: browser.right; leftMargin: ui.gap * 1.6; right: parent.right; rightMargin: ui.gap * 0.6; top: header.bottom; topMargin: ui.px(10); bottom: footer.top; bottomMargin: ui.px(4) }
        }

        Footer {
            id: footer
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        }
    }

    FullStage {
        id: full
        anchors.fill: parent
        opacity: store.full ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }
    }

    // ── what shows over it ──────────────────────────────────────────────
    Scrim { visible: store.help || !!store.pick || !!store.upd || store.barAsk }
    HelpBox { visible: store.help }
    BarAskBox { visible: store.barAsk }
    PickerBox { visible: !!store.pick }
    UpdateBox { visible: !!store.upd }
}
