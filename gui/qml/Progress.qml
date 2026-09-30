// Progress is where the song is: a lit bar that seeks on a click or a
// drag, with a knob to hold, and the times under it.
import QtQuick
import QtQuick.Window
import QtQuick.Effects

Column {
    id: p
    spacing: ui.px(8)

    property double now: Date.now()
    readonly property bool preview: !!store.st.preview
    readonly property double dur: store.st.dur || 0
    readonly property double pos: { store.stateAt; store.seekAt; return store.position(now) }
    property double dragAt: -1
    readonly property double shownPos: dragAt >= 0 ? dragAt : pos
    readonly property bool hot: area.containsMouse || dragAt >= 0

    // The clock ticks as often as the bar grows a pixel of the screen, at
    // most once a second for the times: nothing is drawn that would look
    // the same. While the window's clock runs (ui.clock) it ticks with it,
    // in the frames the rest moves in; else by itself. Hidden, or under a
    // popup's blur, not at all.
    readonly property real dpr: Screen.devicePixelRatio || 1
    readonly property int step: p.dur > 0 ? Math.max(50, Math.min(1000, p.dur * 1000 / Math.max(1, p.width * p.dpr))) : 1000
    readonly property bool ticking: !!store.st.playing && !p.preview && p.visible && !win.popup && win.visibility !== Window.Minimized
    onTickingChanged: if (ticking) now = Date.now()
    Timer {
        interval: p.step
        repeat: true
        running: p.ticking && !ui.pulsing
        onTriggered: p.now = Date.now()
    }
    Connections {
        target: ui
        enabled: p.ticking
        function onClockChanged() {
            const t = Date.now()
            if (t - p.now >= p.step - 25) p.now = t
        }
    }
    Connections { target: store; function onStChanged() { p.now = Date.now() } }

    Item {
        width: parent.width
        height: ui.px(14)

        Rectangle { // the groove
            id: groove
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            height: p.hot ? ui.px(8) : ui.px(6)
            radius: height / 2
            color: Qt.alpha(ui.deep, 0.6)
            border { width: 1; color: ui.edge }
        }

        RectangularShadow { // the glow of what has played
            visible: ui.effects && fill.visible
            x: fill.x
            width: fill.width
            height: fill.height
            anchors.verticalCenter: parent.verticalCenter
            radius: height / 2
            blur: ui.px(10)
            color: Qt.alpha(ui.here, 0.55)
        }
        Rectangle { // what has played, lit
            id: fill
            anchors.verticalCenter: parent.verticalCenter
            height: groove.height
            radius: height / 2
            // A preview reports no position: a lit segment sweeps instead.
            x: p.preview ? sweep.x : 0
            width: p.preview ? parent.width * 0.25
                 : p.dur > 0 ? Math.max(height, Math.round(parent.width * Math.min(1, p.shownPos / p.dur) * p.dpr) / p.dpr) : 0
            visible: p.preview || p.dur > 0
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.darker(ui.here, 1.35) }
                GradientStop { position: 1; color: Qt.lighter(ui.here, 1.15) }
            }
        }
        QtObject { id: sweep; property real x: 0 }
        NumberAnimation {
            target: sweep
            property: "x"
            from: 0
            to: groove.width * 0.75
            duration: 1400
            loops: Animation.Infinite
            running: p.preview && !ui.calm && ui.awake
            easing.type: Easing.InOutSine
        }

        Rectangle { // the knob
            visible: !p.preview && p.dur > 0 && p.hot
            x: fill.x + fill.width - width / 2
            anchors.verticalCenter: parent.verticalCenter
            width: ui.px(14)
            height: width
            radius: width / 2
            color: ui.bright
            border { width: 2; color: ui.here }
        }

        MouseArea {
            id: area
            anchors { fill: parent; margins: -ui.px(6) }
            hoverEnabled: true
            enabled: !p.preview && p.dur > 0
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            function at(x) { return Math.max(0, Math.min(1, x / width)) * p.dur }
            onPressed: e => p.dragAt = at(e.x)
            onPositionChanged: e => { if (pressed) p.dragAt = at(e.x) }
            onReleased: e => { store.seek(at(e.x)); p.dragAt = -1 }
        }
    }

    Item {
        width: parent.width
        height: left.height
        Text {
            id: left
            text: p.preview ? "30 s clip" : clock(p.shownPos)
            color: ui.fg
            font { family: ui.sans; pixelSize: ui.px(12); features: { "tnum": 1 } }
        }
        Text {
            anchors.right: parent.right
            text: p.preview ? (store.st.title ? "then " + store.st.title : "") : p.dur > 0 ? "-" + clock(p.dur - p.shownPos) : ""
            color: ui.dim
            elide: Text.ElideRight
            width: Math.min(implicitWidth, parent.width - left.width - ui.px(20))
            horizontalAlignment: Text.AlignRight
            font { family: ui.sans; pixelSize: ui.px(12); features: { "tnum": 1 } }
        }
    }

    function clock(sec) {
        sec = Math.max(0, Math.floor(sec || 0))
        const h = Math.floor(sec / 3600), m = Math.floor(sec / 60) % 60, s = sec % 60
        return (h ? h + ":" + (m < 10 ? "0" : "") : "") + m + ":" + (s < 10 ? "0" : "") + s
    }
}
