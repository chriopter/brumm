// Lava: a lava lamp. Seven balls of wax drift on slow Lissajous paths, each
// as big as its part of the spectrum; a kick swells them all, the loudness
// hurries the drift, and every eighth kick pours a new color into the lamp.
// One pass (shaders/viz_lava.frag); the drift is kept here, once a frame.
import QtQuick

VizEffect {
    id: fx

    property real beat: 0
    property real phase: 0
    property real speed: 0.25
    property real pulse: 0
    property real swell: 0
    property int kicks: -1
    property int pal: 0
    property int old: 0
    property real mixing: 1

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.beat = a.beat
            fx.speed += (0.25 + a.live * (0.2 + 1.2 * a.level) - fx.speed) * Math.min(1, dt * 1.5)
            fx.phase += dt * fx.speed
            if (a.kicks !== fx.kicks) {
                if (fx.kicks >= 0) fx.pulse = Math.min(fx.pulse + 0.22, 0.4)
                fx.kicks = a.kicks
            }
            fx.pulse *= Math.exp(-dt * 4)
            fx.swell += (fx.pulse - fx.swell) * Math.min(1, dt * 12)
            const p = Math.floor(a.kicks / 8) % 4
            if (p !== fx.pal) { fx.old = fx.pal; fx.pal = p; fx.mixing = 0 }
            if (fx.mixing < 1) fx.mixing = Math.min(1, fx.mixing + dt / 1.5)
        }
    }

    ShaderEffect {
        anchors.fill: parent
        property real phase: fx.phase
        property real swell: fx.swell
        property real aspect: width / Math.max(1, height)
        property real pal: fx.pal
        property real old: fx.old
        property real mixing: fx.mixing
        property real beat: fx.beat
        property color tint: fx.here
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_lava.frag.qsb"
    }
}
