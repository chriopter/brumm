// Lava: a lava lamp, seen from inside its glass. Molten wax in three
// dimensions rises and sinks between a pool on the floor and one under the
// cap: eight balls, each as big as its part of the spectrum, that melt
// together and pull apart in strands. A kick swells them, throws a drop
// off each of the big ones, heaves the floor and flares the bulb; the
// highs make the wax shiver and send bubbles up; caustics dance on the
// back wall with the loudness; every eighth kick pours a new color in.
// Two passes: the wax's shape, marched at half size
// (shaders/viz_lava_wax.frag), and the lamp around it with all its light
// (shaders/viz_lava.frag), bloomed from the shape's mip levels.
import QtQuick

VizEffect {
    id: fx

    property real phase: 0
    property real tick: 0
    property real speed: 0.25
    property real pulse: 0
    property real swell: 0
    property real split: 0
    property int kicks: -1
    property int pal: 0
    property int old: 0
    property real mixing: 1

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.speed += (0.22 + a.live * (0.2 + 1.2 * a.level) - fx.speed) * Math.min(1, dt * 1.5)
            fx.phase += dt * fx.speed
            fx.tick += dt * (0.3 + 0.7 * a.live)
            if (a.kicks !== fx.kicks) {
                if (fx.kicks >= 0) {
                    fx.pulse = Math.min(fx.pulse + 0.22, 0.4)
                    fx.split = Math.min(fx.split + 0.7, 1)
                }
                fx.kicks = a.kicks
            }
            fx.pulse *= Math.exp(-dt * 4)
            fx.split *= Math.exp(-dt * 1.6)
            fx.swell += (fx.pulse - fx.swell) * Math.min(1, dt * 12)
            const p = Math.floor(a.kicks / 8) % 6
            if (p !== fx.pal) { fx.old = fx.pal; fx.pal = p; fx.mixing = 0 }
            if (fx.mixing < 1) fx.mixing = Math.min(1, fx.mixing + dt / 1.5)
        }
    }

    // The wax's shape, drawn into its texture only.
    ShaderEffect {
        id: march
        anchors.fill: parent
        blending: false
        property real phase: fx.phase
        property real tick: fx.tick
        property real aspect: width / Math.max(1, height)
        property real swell: fx.swell
        property real split: fx.split
        property real bass: fx.audio.bass
        property real high: fx.audio.high
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_lava_wax.frag.qsb"
    }
    ShaderEffectSource {
        id: shape
        sourceItem: march
        hideSource: true
        live: fx.running
        mipmap: true
        smooth: true
        textureSize: Qt.size(Math.max(2, Math.round(fx.width / 2)), Math.max(2, Math.round(fx.height / 2)))
    }

    ShaderEffect {
        anchors.fill: parent
        property real phase: fx.phase
        property real tick: fx.tick
        property real aspect: width / Math.max(1, height)
        property real pal: fx.pal
        property real old: fx.old
        property real mixing: fx.mixing
        property real beat: fx.audio.beat
        property real bass: fx.audio.bass
        property real high: fx.audio.high
        property real level: fx.audio.level
        property color tint: fx.here
        property var shape: shape
        fragmentShader: "qrc:/shaders/viz_lava.frag.qsb"
    }
}
