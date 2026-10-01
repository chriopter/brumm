// Fire: an inferno over a floor of black glass. Flames stand on the floor,
// each column as high as its part of the spectrum (the bass in the
// middle), and a curling wind carries their heat up; every kick shoots a
// fireball up on a jet and throws a wave of embers, a strong one sends a
// shock ring out; smoke hangs above, lit from below; the floor mirrors it
// between cracks of lava that pulse with the bass. Every sixteen kicks the
// fire changes its gas: blue, purple, green, and back.
//
// Two passes: the burning at half size over its own last frame
// (shaders/viz_fire_sim.frag), then the picture (shaders/viz_fire.frag).
// The few numbers they share are kept here, once a frame.
import QtQuick

VizEffect {
    id: fx

    property real time: 0
    property real dt: 0
    property real rise1: 0
    property real rise2: 0
    property real flare: 0
    property real reach: 0.15

    // The last four kicks: seconds since each, where its fireball went up, how strong.
    property vector4d ages: Qt.vector4d(99, 99, 99, 99)
    property vector4d ballx: Qt.vector4d(0.5, 0.5, 0.5, 0.5)
    property vector4d ballpow: Qt.vector4d(0, 0, 0, 0)
    property int seen: -1
    property int count: 0

    property real blastAge: 99
    property real blastPow: 0

    // The gas: 0 fire, 1 blue, 2 green, 3 purple; fire between the others.
    readonly property var gases: [0, 1, 0, 3, 0, 2]
    property real huesFrom: 0
    property real huesTo: 0
    property real huesMix: 1

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.dt = dt
            fx.time = (fx.time + dt) % 4096
            fx.flare += (a.beat - fx.flare) * Math.min(1, dt * 14)
            fx.reach = 0.14 + 0.34 * Math.min(1, a.level * 1.8) + 0.25 * fx.flare
            const rise = 0.7 + 0.6 * a.level + 0.5 * fx.flare // heights a second
            fx.rise1 = (fx.rise1 + dt * rise * 3.5) % 256
            fx.rise2 = (fx.rise2 + dt * rise * 10.5) % 256

            const ages = [fx.ages.x, fx.ages.y, fx.ages.z, fx.ages.w].map(v => Math.min(99, v + dt))
            if (fx.seen < 0) fx.seen = a.kicks
            if (a.kicks !== fx.seen) {
                fx.seen = a.kicks
                fx.count++
                const big = fx.count % 4 === 0
                const pow = Math.min(1, 0.3 + 0.5 * a.bass + (big ? 0.35 : 0))
                const slot = fx.count % 4
                const xs = [fx.ballx.x, fx.ballx.y, fx.ballx.z, fx.ballx.w]
                const ps = [fx.ballpow.x, fx.ballpow.y, fx.ballpow.z, fx.ballpow.w]
                ages[slot] = 0
                xs[slot] = 0.1 + 0.8 * Math.random()
                ps[slot] = pow
                fx.ballx = Qt.vector4d(xs[0], xs[1], xs[2], xs[3])
                fx.ballpow = Qt.vector4d(ps[0], ps[1], ps[2], ps[3])
                if (big || a.bass > 0.6) { fx.blastAge = 0; fx.blastPow = pow }
                if (fx.count % 16 === 0) {
                    fx.huesFrom = fx.huesTo
                    fx.huesTo = fx.gases[(fx.count / 16) % fx.gases.length]
                    fx.huesMix = 0
                }
            }
            fx.ages = Qt.vector4d(ages[0], ages[1], ages[2], ages[3])
            fx.blastAge = Math.min(99, fx.blastAge + dt)
            fx.huesMix = Math.min(1, fx.huesMix + dt * 1.5)
        }
    }

    // The burning over its own last frame, at half size.
    ShaderEffect {
        id: burn
        width: fx.width
        height: fx.height
        property real time: fx.time
        property real dt: fx.dt
        property real aspect: width / Math.max(1, height)
        property real px: 2 / Math.max(1, height)
        property real reach: fx.reach
        property real rise1: fx.rise1
        property real rise2: fx.rise2
        property real flare: fx.flare
        property real level: fx.audio.level
        property real embers: fx.audio.live * (0.05 + 0.3 * fx.audio.level + 0.3 * fx.audio.high)
        property vector4d ages: fx.ages
        property vector4d ballx: fx.ballx
        property vector4d ballpow: fx.ballpow
        property var prev: heat
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_fire_sim.frag.qsb"
    }
    ShaderEffectSource {
        id: heat
        sourceItem: burn
        hideSource: true
        visible: false
        recursive: true
        live: fx.running
        mipmap: true
        smooth: true
        textureSize: Qt.size(Math.max(1, fx.width / 2), Math.max(1, fx.height / 2))
    }

    // The picture: the fire colored and bloomed, the smoke, the floor.
    ShaderEffect {
        anchors.fill: parent
        property real time: fx.time
        property real aspect: width / Math.max(1, height)
        property real rise2: fx.rise2
        property real flare: fx.flare
        property real bass: fx.audio.bass
        property real level: fx.audio.level
        property real blastAge: fx.blastAge
        property real blastPow: fx.blastPow
        property real huesFrom: fx.huesFrom
        property real huesTo: fx.huesTo
        property real huesMix: fx.huesMix
        property color tint: fx.here
        property var sim: heat
        fragmentShader: "qrc:/shaders/viz_fire.frag.qsb"
    }
}
