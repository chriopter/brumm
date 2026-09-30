// Scope: an oscilloscope. The waveform is traced by a white-hot beam onto
// a phosphor that flashes blue and fades green over a third of a second,
// over a glowing graticule behind curved glass. A trigger on a rising zero
// crossing holds periodic sounds still, and an auto-gain fills the screen
// with quiet passages too; kicks thicken the beam. Without sound (or
// paused) it traces a calm signal made from the spectrum, shrunk to a
// ripple. Three passes: the trigger and gain in a single pixel that keeps
// them from frame to frame (shaders/viz_scope_trig.frag), the phosphor at
// half size (shaders/viz_scope_trail.frag) and the screen, bloomed from
// its mip levels (shaders/viz_scope.frag).
import QtQuick

VizEffect {
    id: fx

    property real dt: 0

    // a theme color at full light
    function lit(c, s) {
        const q = Qt.color(c || fx.accent)
        return Qt.hsva(q.hsvHue < 0 ? 0 : q.hsvHue, Math.min(1, q.hsvSaturation * s + 0.15), 1, 1)
    }

    FrameAnimation {
        running: fx.running
        onTriggered: fx.dt = Math.min(frameTime, 0.1)
    }

    // The trigger and the gain, one pixel: r where the trace starts in the
    // wave, g and b the gain (high and low byte), a 1 for sound, 0 for
    // the made-up signal.
    ShaderEffect {
        id: trig
        width: 1
        height: 1
        blending: false
        property real live: fx.audio.live
        property real ease: Math.min(1, fx.dt * 4)
        property var prev: scanTex
        property var wave: fx.wave
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_scope_trig.frag.qsb"
    }
    ShaderEffectSource {
        id: scanTex
        sourceItem: trig
        hideSource: true
        recursive: true
        live: fx.running
        smooth: false
        textureSize: Qt.size(1, 1)
    }

    // The phosphor: two afterglows, a quick and a slow, fading under the
    // beam.
    ShaderEffect {
        id: trail
        anchors.fill: parent
        blending: false
        property real time: fx.audio.t
        property real beat: fx.audio.beat
        property real quick: Math.exp(-fx.dt * 16)
        property real slow: Math.exp(-fx.dt * 5)
        property size px: Qt.size(width / 2, height / 2)
        property var prev: phosphorTex
        property var scan: scanTex
        property var wave: fx.wave
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_scope_trail.frag.qsb"
    }
    ShaderEffectSource {
        id: phosphorTex
        sourceItem: trail
        hideSource: true
        recursive: true
        live: fx.running
        mipmap: true
        smooth: true
        textureSize: Qt.size(Math.max(2, Math.round(fx.width / 2)), Math.max(2, Math.round(fx.height / 2)))
    }

    ShaderEffect {
        anchors.fill: parent
        property real time: fx.audio.t
        property real beat: fx.audio.beat
        property real bass: fx.audio.bass
        property size px: Qt.size(width, height)
        property color ground: theme.dark ? (fx.colors.darker_background || "#0e0e14") : "#0c0c14"
        property color flash: fx.lit(fx.colors.cyan, 1.1)
        property color glow: fx.lit(fx.colors.green, 1.3)
        property color halo: fx.lit(fx.here, 1.2)
        property var phosphor: phosphorTex
        property var scan: scanTex
        property var wave: fx.wave
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_scope.frag.qsb"
    }
}
