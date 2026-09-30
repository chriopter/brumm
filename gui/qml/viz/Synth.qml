// Synth is the terminal's synthwave: a striped sun sinking behind two
// ranges of mountains raised by the spectrum, over a neon grid that races
// at the loudness's pace. One pass, drawn by shaders/viz_synth.frag.
import QtQuick

VizEffect {
    id: fx

    // What moves, eased once a frame: the beat, the grid's pace and how far
    // it has come (wrapped at a row), the sun's stripes.
    property real flare: 0
    property real speed: 0.3
    property real run: 0
    property real stripe: 0
    property real clock: 0
    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.flare += (a.beat - fx.flare) * Math.min(1, dt * 12)
            const target = 0.3 + a.live * (0.4 + 2.6 * a.level) + 1.2 * fx.flare
            fx.speed += (target - fx.speed) * Math.min(1, dt * 3)
            fx.run = (fx.run + dt * fx.speed) % 0.6
            fx.stripe = (fx.stripe + dt * (0.2 + 0.5 * a.level)) % 1
            fx.clock = a.t
        }
    }

    ShaderEffect {
        anchors.fill: parent
        property real time: fx.clock
        property real aspect: width / Math.max(1, height)
        property real run: fx.run
        property real stripe: fx.stripe
        property real flare: fx.flare
        property real bass: fx.audio.bass
        property real high: fx.audio.high
        property real level: fx.audio.level
        property real live: fx.audio.live
        property color tint: fx.here
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_synth.frag.qsb"
    }
}
