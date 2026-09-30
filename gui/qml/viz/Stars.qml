// stars: a warp starfield. Stars fly out of the center while the field
// slowly rolls, the speed eased toward the loudness and kicked into
// hyperspace on every beat, when the streaks stretch long and the tunnel
// lights up. Far stars glow deep blue, near ones burn white.
import QtQuick

VizEffect {
    id: fx

    // The flight, moved on once a frame: how fast, how far, how rolled.
    property real speed: 0.06
    property real travel: 0
    property real roll: 0
    property real time: 0
    property real beat: 0
    property real level: 0
    property real last: -1

    Connections {
        target: fx.running ? fx.audio : null
        function onTicked() {
            const a = fx.audio
            const dt = fx.last < 0 ? 0 : Math.min(0.1, Math.max(0, a.t - fx.last))
            fx.last = a.t
            const target = 0.06 + a.live * (0.25 + 1.6 * a.level) + 3.5 * a.beat
            fx.speed += (target - fx.speed) * Math.min(1, dt * 5)
            fx.travel += fx.speed * dt
            fx.roll += dt * (0.03 + 0.3 * a.level)
            fx.time += dt
            fx.beat = a.beat
            fx.level = a.level
        }
    }
    onRunningChanged: last = -1

    ShaderEffect {
        anchors.fill: parent
        property real time: fx.time
        property real travel: fx.travel
        property real roll: fx.roll
        property real speed: fx.speed
        property real beat: fx.beat
        property real level: fx.level
        property real aspect: width / Math.max(1, height)
        property real pixel: 2 / Math.max(1, height)
        property color tint: fx.here
        fragmentShader: "qrc:/shaders/viz_stars.frag.qsb"
    }
}
