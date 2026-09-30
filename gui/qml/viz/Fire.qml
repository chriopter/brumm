// Fire: flames across the floor, each column as high as its part of the
// spectrum (the bass in the middle). Loudness makes them reach higher and
// rise faster, a kick flares them and throws sparks. One pass
// (shaders/viz_fire.frag); the rise is kept here, once a frame.
import QtQuick

VizEffect {
    id: fx

    property real time: 0
    property real rise1: 0
    property real rise2: 0
    property real flare: 0
    property real reach: 0.3
    property real sparks: 0

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.time += dt
            fx.flare += (a.beat - fx.flare) * Math.min(1, dt * 14)
            fx.reach = 0.28 + 0.5 * Math.min(1, a.level * 1.8) + 0.3 * fx.flare
            const rise = 0.7 + 0.6 * a.level + 0.5 * fx.flare // heights a second
            fx.rise1 = (fx.rise1 + dt * rise * 3.5) % 256
            fx.rise2 = (fx.rise2 + dt * rise * 10.5) % 256
            fx.sparks = a.live * (0.25 * a.level + 0.9 * a.beat)
        }
    }

    ShaderEffect {
        anchors.fill: parent
        property real time: fx.time
        property real aspect: width / Math.max(1, height)
        property real reach: fx.reach
        property real rise1: fx.rise1
        property real rise2: fx.rise2
        property real sparks: fx.sparks
        property real flare: fx.flare
        property color tint: fx.here
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_fire.frag.qsb"
    }
}
