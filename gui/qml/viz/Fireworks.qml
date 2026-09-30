// Fireworks: shells launched by the kicks (and now and then by the mids)
// climb and burst — peonies, rings, gold willows, two-color crossettes —
// over a harbor city that lights up with them. The shells are a dozen
// numbers kept here; the sparks are drawn at half size over their own last
// frame, so they leave trails, then glowed, mirrored and lit at full size.
import QtQuick

VizEffect {
    id: fx

    // Twelve shells, newest over oldest: (x, apex, launched, code).
    property vector4d s0; property vector4d s1; property vector4d s2; property vector4d s3
    property vector4d s4; property vector4d s5; property vector4d s6; property vector4d s7
    property vector4d s8; property vector4d s9; property vector4d s10; property vector4d s11
    property int next: 0
    property int kicks: -1
    property real acc: 0
    property real time: 0
    property real dt: 0

    // launch puts a shell up, somewhere over the city; small ones lower.
    function launch(small) {
        const apex = (0.5 + 0.38 * Math.random()) * (small ? 0.8 : 1)
        const code = Math.floor(Math.random() * 100000) + Math.min(0.99, audio.bass * 0.7 + audio.beat * 0.3) * (small ? 0.4 : 1)
        fx["s" + next] = Qt.vector4d(0.1 + 0.8 * Math.random(), apex, time, code)
        next = (next + 1) % 12
    }

    Connections {
        target: fx.audio
        enabled: fx.running
        function onTicked() {
            const a = fx.audio
            fx.dt = Math.min(Math.max(a.t - fx.time, 0), 0.1)
            fx.time = a.t
            if (fx.kicks < 0) fx.kicks = a.kicks
            if (a.kicks !== fx.kicks) {
                fx.kicks = a.kicks
                fx.launch(false)
                if (a.bass > 0.55) fx.launch(false)
            }
            fx.acc += fx.dt * (0.3 + 1.6 * a.mid * a.live + 0.3 * (1 - a.live))
            while (fx.acc >= 1) { fx.acc -= 1; fx.launch(a.live < 0.5) }
        }
    }

    // The sparks over their own last frame, at half size.
    ShaderEffect {
        id: sparks
        width: fx.width
        height: fx.height
        property real time: fx.time
        property real dt: fx.dt
        property real aspect: width / Math.max(1, height)
        property real px: 2 / Math.max(1, height)
        property real glitter: fx.audio.high
        property color tint: fx.here
        property vector4d s0: fx.s0; property vector4d s1: fx.s1; property vector4d s2: fx.s2; property vector4d s3: fx.s3
        property vector4d s4: fx.s4; property vector4d s5: fx.s5; property vector4d s6: fx.s6; property vector4d s7: fx.s7
        property vector4d s8: fx.s8; property vector4d s9: fx.s9; property vector4d s10: fx.s10; property vector4d s11: fx.s11
        property var prev: trail
        fragmentShader: "qrc:/shaders/viz_fireworks_sparks.frag.qsb"
    }
    ShaderEffectSource {
        id: trail
        sourceItem: sparks
        hideSource: true
        visible: false
        recursive: true
        live: fx.running
        mipmap: true
        smooth: true
        textureSize: Qt.size(Math.max(1, fx.width / 2), Math.max(1, fx.height / 2))
    }

    // The night, the city, the water, and the sparks glowing over them.
    ShaderEffect {
        anchors.fill: parent
        property real time: fx.time
        property real aspect: width / Math.max(1, height)
        property real beat: fx.audio.beat
        property color tint: fx.here
        property vector4d s0: fx.s0; property vector4d s1: fx.s1; property vector4d s2: fx.s2; property vector4d s3: fx.s3
        property vector4d s4: fx.s4; property vector4d s5: fx.s5; property vector4d s6: fx.s6; property vector4d s7: fx.s7
        property vector4d s8: fx.s8; property vector4d s9: fx.s9; property vector4d s10: fx.s10; property vector4d s11: fx.s11
        property var sparks: trail
        fragmentShader: "qrc:/shaders/viz_fireworks.frag.qsb"
    }
}
