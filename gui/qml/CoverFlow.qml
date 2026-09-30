// CoverFlow is the stage's cover among its neighbours: what plays stands
// flat in the middle (a CoverCard, a click turns it over), the songs that
// played lean away to its left and the ones coming next to its right,
// all on a glossy floor, fading out toward the sides. A click on one to
// the right plays it. Still but for a short slide when the song changes.
import QtQuick
import QtQuick.Effects

Item {
    id: flow
    property int size: ui.px(300)   // the middle cover's side
    property string url
    readonly property real reflection: 0.2
    readonly property int gap: ui.px(4)
    readonly property real strength: theme.dark ? 0.24 : 0.35
    // The neighbours: a little smaller, turned, the first one tucked
    // close to the middle, the rest stacked tighter.
    readonly property real sideSize: size * 0.72
    readonly property real angle: 60
    readonly property real first: size * 0.5 + sideSize * 0.12
    readonly property real step: sideSize * 0.2

    height: Math.round(size * (1 + reflection) + gap)

    // Played before, the latest nearest; coming next, the soonest nearest.
    // (Built from the playing song's id, not from store.st: that is new
    // with every report of the player, and the covers are not.)
    readonly property var lefts: store.recent.filter(t => t.id !== playing).slice(-4)
    readonly property var rights: {
        const i = store.upNext.findIndex(t => t.id === playing) // not asked again yet
        return store.upNext.slice(i + 1, i + 6).map((t, n) => Object.assign({ at: i + 1 + n }, t))
    }

    // A song change slides everything along by the covers it moved.
    property real shift: 0
    readonly property string playing: store.st.id || ""
    onPlayingChanged: {
        const i = store.upNext.findIndex(t => t.id === playing)
        shift = i >= 0 ? Math.min(i + 1, 3) : store.recent.slice(0, -1).some(t => t.id === playing) ? -1 : 0
        if (shift !== 0 && !ui.calm) slide.restart()
        else shift = 0
    }
    NumberAnimation { id: slide; target: flow; property: "shift"; to: 0; duration: 320; easing.type: Easing.OutCubic }

    // The floor: a line of light where the covers stand, fading out at
    // both ends.
    Rectangle {
        y: flow.size + Math.round(flow.gap / 2)
        anchors { left: parent.left; right: parent.right }
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.5; color: Qt.alpha(ui.here, 0.35) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    Repeater {
        model: flow.lefts.map((t, i) => ({ t: t, k: i - flow.lefts.length }))
            .concat(flow.rights.map((t, i) => ({ t: t, k: i + 1 })))
        ShaderEffect {
            id: cov
            required property var modelData
            // Where it stands: 0 is the middle, ±1 the first to a side.
            readonly property real d: modelData.k + flow.shift
            readonly property real a: Math.abs(d)
            readonly property real towards: d < 0 ? -1 : 1
            readonly property real near: Math.min(1, a)
            readonly property real off: towards * (near * flow.first + Math.max(0, a - 1) * flow.step)
            // Fading out as it stands farther off and nears the edge.
            readonly property real room: (flow.width / 2 - Math.abs(off) - side * 0.2) / (side * 0.22)

            property real side: flow.size + (flow.sideSize - flow.size) * near
            property real radius: ui.px(8)
            property real reflection: flow.reflection
            property real gap: flow.gap
            property real strength: flow.strength
            property real shade: Math.min(1, near * 0.45 + Math.max(0, a - 1) * 0.2)
            property var source: art
            fragmentShader: "qrc:/shaders/flow.frag.qsb"

            x: Math.round(flow.width / 2 + off - side / 2)
            y: flow.size - side
            z: -a
            width: side
            height: side * (1 + reflection) + gap
            opacity: Math.max(0, Math.min(1, room)) * Math.max(0, 1 - Math.max(0, a - 1) * 0.18)
            visible: ui.effects && opacity > 0.01
            transform: Rotation {
                origin { x: cov.width / 2; y: cov.side } // on the floor: it and its reflection mirror there
                axis { x: 0; y: 1; z: 0 }
                angle: -cov.towards * flow.angle * cov.near
            }

            Image {
                id: art
                visible: false
                source: cov.modelData.t.artwork ? "image://cover/" + encodeURIComponent(cov.modelData.t.artwork) : ""
                sourceSize: Qt.size(ui.px(256), ui.px(256))
                asynchronous: true
                fillMode: Image.PreserveAspectCrop
                mipmap: true
            }
            HoverHandler { enabled: cov.modelData.k > 0; cursorShape: Qt.PointingHandCursor }
            TapHandler { enabled: cov.modelData.k > 0; onTapped: store.jumpTo(cov.modelData.t.at) }
        }
    }

    // The middle one lit from behind in its own color, as the XMB lights
    // the card of what is chosen.
    RectangularShadow {
        visible: ui.effects && flow.url !== ""
        z: -10
        x: Math.round((flow.width - flow.size) / 2)
        width: flow.size
        height: flow.size
        radius: ui.coverRadius
        blur: flow.size * 0.18
        spread: flow.size * 0.02
        color: Qt.alpha(ui.here, 0.4)
    }

    CoverCard {
        z: 1
        x: Math.round((flow.width - flow.size) / 2)
        width: flow.size
        height: flow.size
        url: flow.url
        reflect: true
        reflection: flow.reflection
    }
}
