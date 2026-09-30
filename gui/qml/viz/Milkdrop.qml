// Milkdrop: a ring of light whose radius is the spectrum (mirrored, so it
// is whole) and a ring inside it bent by the waveform, drawn every frame
// into a feedback texture that zooms out, twists and fades, so their
// echoes spiral outward and slowly turn color. A kick punches the zoom and
// fires a shockwave; every eighth kick reverses the twist, every fourth
// changes the colors. Two passes: the feedback at half size
// (shaders/viz_milkdrop_warp.frag) and the screen, bloomed from its mip
// levels (shaders/viz_milkdrop.frag).
import QtQuick

VizEffect {
    id: fx

    property real rot: 0
    property real steps: 0
    readonly property real dir: Math.floor(audio.kicks / 8) % 2 ? -1 : 1
    readonly property var pal: [colors.magenta, colors.cyan, colors.blue, colors.green, colors.yellow, colors.red]
    readonly property int hi: Math.floor(audio.t / 15) + Math.floor(audio.kicks / 4)

    // a theme color at full light: the rings glow in it
    function lit(c) {
        const q = Qt.color(c || fx.accent)
        return Qt.hsva(q.hsvHue < 0 ? 0 : q.hsvHue, Math.min(1, q.hsvSaturation * 1.25 + 0.1), 1, 1)
    }

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.rot += fx.dir * dt * (0.25 + 1.5 * a.level)
            fx.steps = dt * 30
        }
    }

    // One step of the echoes, drawn into the feedback texture only.
    ShaderEffect {
        id: warp
        anchors.fill: parent
        property real time: fx.audio.t
        property real aspect: width / Math.max(1, height)
        property real steps: fx.steps
        property real rot: fx.rot
        property real twist: 0.025 * fx.dir
        property real bass: fx.audio.bass
        property real beat: fx.audio.beat
        property real level: fx.audio.level
        property real live: fx.audio.live
        property real since: fx.audio.since
        property color hueA: fx.lit(fx.pal[fx.hi % 6])
        property color hueB: fx.lit(fx.pal[(fx.hi + 2) % 6])
        property color flash: Qt.tint(fx.lit(fx.here), "#80ffffff")
        property var prev: echoes
        property var spectrum: fx.spectrum
        property var wave: fx.wave
        fragmentShader: "qrc:/shaders/viz_milkdrop_warp.frag.qsb"
    }
    ShaderEffectSource {
        id: echoes
        sourceItem: warp
        hideSource: true
        recursive: true
        live: fx.running
        mipmap: true
        smooth: true
        textureSize: Qt.size(Math.max(2, Math.round(fx.width / 2)), Math.max(2, Math.round(fx.height / 2)))
    }

    ShaderEffect {
        anchors.fill: parent
        property real aspect: width / Math.max(1, height)
        property real beat: fx.audio.beat
        property real bass: fx.audio.bass
        property color ground: fx.colors.darker_background || "#0e0e14"
        property color glow: fx.lit(fx.here)
        property var feed: echoes
        fragmentShader: "qrc:/shaders/viz_milkdrop.frag.qsb"
    }
}
