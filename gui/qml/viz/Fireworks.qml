// Fireworks: a grand finale over a harbor city. Every kick throws mines up
// from the water at once and sends a shell climbing that bursts on a later
// beat — chrysanthemums, saturn rings, gold willows, crossettes, hearts,
// pinwheels, shells of shells, palms — as big as the bass; a strong kick
// sends a barrage, every sixteenth a whole sky of them. The highs set old
// stars crackling, the spectrum lights the city's windows, the smoke hangs
// and is lit by the next bursts, and the water mirrors it all.
//
// Three passes: the sparks at half size over their own last frame, so they
// leave trails (shaders/viz_fireworks_sparks.frag); the smoke at quarter
// size over its own (viz_fireworks_smoke.frag); then the picture
// (viz_fireworks.frag). The shells are twenty rows of four numbers, kept
// here.
import QtQuick

VizEffect {
    id: fx

    // Twenty shells, newest over oldest, four to a matrix, each a row:
    // (x, apex + 50 × the climb's seconds, the second it bursts, code).
    property var shells: new Array(80).fill(0)
    property matrix4x4 m0; property matrix4x4 m1; property matrix4x4 m2
    property matrix4x4 m3; property matrix4x4 m4
    property int next: 0
    property bool dirty: false
    property int seen: -1
    property int count: 0
    property real acc: 0
    property real last: -1
    property real time: 1
    property real dt: 0
    property real kickAt: -9
    property real punch: 0
    property real period: 0.5  // seconds between kicks, as heard
    // The trails sink a whole texel at a time.
    property real sag: 0
    property real sink: 0

    // launch sends a shell up so that it bursts after `wait` seconds:
    // size 0–1, at x (or anywhere, if negative), of a kind (or any).
    function launch(size, wait, x, kind) {
        const climb = Math.max(0.5, Math.min(1.4, wait))
        const apex = Math.min(0.9, 0.52 + 0.34 * Math.random() + 0.06 * size)
        if (kind < 0) kind = Math.floor(Math.random() * 8)
        const code = Math.floor(Math.random() * 49) * 8 + kind + Math.max(0.05, Math.min(0.98, size))
        const i = next * 4
        shells[i] = x < 0 ? 0.08 + 0.84 * Math.random() : x
        shells[i + 1] = apex + Math.round(climb * 50)
        shells[i + 2] = time + wait
        shells[i + 3] = code
        next = (next + 1) % 20
        dirty = true
    }

    Connections {
        target: fx.audio
        enabled: fx.running
        function onTicked() {
            const a = fx.audio
            fx.dt = fx.last < 0 ? 0 : Math.min(Math.max(a.t - fx.last, 0), 0.1)
            fx.last = a.t
            fx.time += fx.dt
            if (fx.time > 4096) {
                // start the clock over, before the numbers get coarse
                fx.time = 1; fx.kickAt = -9
                fx.shells.fill(0); fx.dirty = true
            }
            fx.sag += fx.dt * 0.022 * fx.height / 2
            fx.sink = Math.floor(fx.sag)
            fx.sag -= fx.sink
            if (fx.seen < 0) fx.seen = a.kicks
            if (a.kicks !== fx.seen) {
                fx.seen = a.kicks
                fx.count++
                const gap = fx.time - fx.kickAt
                if (gap > 0.25 && gap < 1.2) fx.period += (gap - fx.period) * 0.3
                fx.kickAt = fx.time
                fx.punch = Math.min(1, 0.25 + 0.6 * a.bass + 0.25 * a.beat)
                // burst on a beat to come: the climb takes whole beats
                const p = fx.period
                const wait = p * Math.ceil(0.8 / p)
                const size = Math.min(1, 0.3 + 0.75 * a.bass)
                if (fx.count % 16 === 0) {
                    // the finale: nine across the sky, a sixteenth apart, a willow over all
                    const order = [4, 1, 7, 3, 5, 0, 8, 2, 6]
                    for (let i = 0; i < 9; i++)
                        fx.launch(0.7 + 0.3 * Math.random(), wait + i * p / 4, 0.1 + 0.1 * order[i], i % 3 === 0 ? 0 : -1)
                    fx.launch(0.98, wait + p * 2.5, 0.5, 2)
                } else if (fx.count % 16 === 1 || fx.count % 16 === 2) {
                    // leave the finale its sky
                } else {
                    fx.launch(size, wait, -1, -1)
                    if (a.bass > 0.55 || fx.count % 4 === 0) {
                        // a barrage: two more, one an eighth behind
                        fx.launch(size * 0.8, wait, -1, -1)
                        fx.launch(size * 0.7, wait + p / 2, -1, -1)
                    }
                }
            }
            // the mids send small ones between the beats; a paused song a few
            fx.acc += fx.dt * (1.5 * a.mid * a.live + 0.35 * (1 - a.live))
            while (fx.acc >= 1) { fx.acc -= 1; fx.launch(0.15 + 0.25 * Math.random(), 0.85, -1, -1) }
            if (fx.dirty) {
                fx.dirty = false
                const s = fx.shells
                fx.m0 = Qt.matrix4x4(s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11], s[12], s[13], s[14], s[15])
                fx.m1 = Qt.matrix4x4(s[16], s[17], s[18], s[19], s[20], s[21], s[22], s[23], s[24], s[25], s[26], s[27], s[28], s[29], s[30], s[31])
                fx.m2 = Qt.matrix4x4(s[32], s[33], s[34], s[35], s[36], s[37], s[38], s[39], s[40], s[41], s[42], s[43], s[44], s[45], s[46], s[47])
                fx.m3 = Qt.matrix4x4(s[48], s[49], s[50], s[51], s[52], s[53], s[54], s[55], s[56], s[57], s[58], s[59], s[60], s[61], s[62], s[63])
                fx.m4 = Qt.matrix4x4(s[64], s[65], s[66], s[67], s[68], s[69], s[70], s[71], s[72], s[73], s[74], s[75], s[76], s[77], s[78], s[79])
            }
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
        property real since: fx.time - fx.kickAt
        property real punch: fx.punch
        property real kick: fx.count % 512
        property real sink: fx.sink
        property color tint: fx.here
        property matrix4x4 m0: fx.m0; property matrix4x4 m1: fx.m1; property matrix4x4 m2: fx.m2
        property matrix4x4 m3: fx.m3; property matrix4x4 m4: fx.m4
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

    // The smoke over its own last frame, at quarter size.
    ShaderEffect {
        id: smoke
        width: fx.width
        height: fx.height
        property real time: fx.time
        property real dt: fx.dt
        property real aspect: width / Math.max(1, height)
        property var prev: haze
        property var sparks: trail
        fragmentShader: "qrc:/shaders/viz_fireworks_smoke.frag.qsb"
    }
    ShaderEffectSource {
        id: haze
        sourceItem: smoke
        hideSource: true
        visible: false
        recursive: true
        live: fx.running
        mipmap: true
        smooth: true
        textureSize: Qt.size(Math.max(1, fx.width / 4), Math.max(1, fx.height / 4))
    }

    // The night, the city, the water, and the sparks glowing over them.
    ShaderEffect {
        anchors.fill: parent
        property real time: fx.time
        property real aspect: width / Math.max(1, height)
        property real beat: fx.audio.beat
        property real glitter: fx.audio.high
        property real level: fx.audio.level
        property color tint: fx.here
        property matrix4x4 m0: fx.m0; property matrix4x4 m1: fx.m1; property matrix4x4 m2: fx.m2
        property matrix4x4 m3: fx.m3; property matrix4x4 m4: fx.m4
        property var sparks: trail
        property var smoke: haze
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_fireworks.frag.qsb"
    }
}
