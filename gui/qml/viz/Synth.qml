// Synth is the terminal's synthwave, flown: the camera races down a wet
// neon road toward a striped sun, between hills raised by the spectrum
// (the bass towering at the flanks), banking and bobbing with the bass.
// A kick swells the sun, throws its rays and parts the colors; the highs
// send a shooting star; every eighth kick the tape slips; every
// sixteenth the neon turns to the theme's next colors. Three passes: the
// world (shaders/viz_synth_scene.frag), the sun's rays at a quarter of
// the size (shaders/viz_synth_rays.frag) and the screen, bloomed from the
// world's mip levels (shaders/viz_synth.frag).
import QtQuick

VizEffect {
    id: fx

    // one lap of the land: its hills and rows repeat after it
    readonly property real lap: 62.832

    // What moves, eased once a frame: the beat, the flight's pace and how
    // far it has come, the sun's stripes, the camera, the shooting star,
    // the tape's slip, the neon's two colors.
    property real flare: 0
    property real speed: 1.2
    property real run: 0
    property real stripe: 0
    property real clock: 0
    property real bass: 0
    property real high: 0
    property real level: 0
    property real roll: 0
    property real dip: 0
    property real sway: 0
    property real alt: 1
    property real shoot: 1
    property real shootSeed: 0
    property real glitch: 0
    property int kicks: -1
    property real highWas: 0
    property color hot: lit(colors.magenta)
    property color ice: lit(colors.cyan)
    property point sun: Qt.point(0.5, 0.45)

    readonly property var hots: [colors.magenta, colors.red, colors.yellow]
    readonly property var ices: [colors.cyan, colors.blue, colors.green, colors.magenta]

    // a theme color at full light: neon glows in it
    function lit(c) {
        const q = Qt.color(c || fx.accent)
        return Qt.hsva(q.hsvHue < 0 ? 0 : q.hsvHue, Math.min(1, q.hsvSaturation * 1.3 + 0.15), 1, 1)
    }
    function toward(c, to, k) {
        return Qt.rgba(c.r + (to.r - c.r) * k, c.g + (to.g - c.g) * k, c.b + (to.b - c.b) * k, 1)
    }

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.flare += (a.beat - fx.flare) * Math.min(1, dt * 14)
            const target = 1.4 + a.live * (0.8 + 5 * a.level) + 2.5 * fx.flare
            fx.speed += (target - fx.speed) * Math.min(1, dt * 3)
            fx.run = (fx.run + dt * fx.speed) % fx.lap
            fx.stripe = (fx.stripe + dt * (0.2 + 0.5 * a.level)) % 1
            fx.clock = a.t
            fx.bass = a.bass
            fx.high = a.high
            fx.level = a.level

            // the camera: a slow weave across the road, banking into it,
            // deeper with the bass; a kick lifts it and tips the horizon
            const weave = Math.sin(a.t * 0.37) + 0.5 * Math.sin(a.t * 0.83 + 1)
            const k = Math.min(1, dt * 5)
            fx.sway += (0.55 * weave - fx.sway) * k
            fx.roll += ((Math.cos(a.t * 0.37) + 0.5 * Math.cos(a.t * 0.83 + 1)) * (0.035 + 0.1 * a.bass) - fx.roll) * k
            fx.alt += (1 + 0.22 * a.bass + 0.12 * fx.flare - fx.alt) * k
            fx.dip += (0.035 * a.bass - 0.03 * fx.flare - fx.dip) * k

            // where the sun stands in the turned picture
            const asp = fx.width / Math.max(1, fx.height)
            const qy = 0.06 + fx.dip - 0.35 * Math.min(0.347, asp * 0.2)
            fx.sun = Qt.point(0.5 + Math.sin(fx.roll) * qy / asp, 0.5 + Math.cos(fx.roll) * qy)

            // a shooting star when the highs leap
            if (fx.shoot >= 1 && a.high > 0.35 && a.high > fx.highWas + 0.04) {
                fx.shoot = 0
                fx.shootSeed = Math.random()
            } else if (fx.shoot < 1) {
                fx.shoot = Math.min(1, fx.shoot + dt * 1.3)
            }
            fx.highWas += (a.high - fx.highWas) * Math.min(1, dt * 4)

            // every eighth kick the tape slips
            if (a.kicks !== fx.kicks) {
                if (fx.kicks >= 0 && a.kicks % 8 === 0)
                    fx.glitch = 1
                fx.kicks = a.kicks
            }
            fx.glitch = Math.max(0, fx.glitch - dt * 3.2)

            // every sixteenth the neon turns to the next colors
            const turn = Math.floor(a.kicks / 16)
            const e = Math.min(1, dt * 1.5)
            fx.hot = fx.toward(fx.hot, fx.lit(fx.hots[turn % 3]), e)
            fx.ice = fx.toward(fx.ice, fx.lit(fx.ices[turn % 4]), e)
        }
    }

    // The world, drawn into a texture only.
    ShaderEffect {
        id: world
        anchors.fill: parent
        blending: false
        property real time: fx.clock
        property real aspect: width / Math.max(1, height)
        property real run: fx.run
        property real stripe: fx.stripe
        property real flare: fx.flare
        property real bass: fx.bass
        property real high: fx.high
        property real level: fx.level
        property real roll: fx.roll
        property real dip: fx.dip
        property real sway: fx.sway
        property real alt: fx.alt
        property real shoot: fx.shoot
        property real shootSeed: fx.shootSeed
        property color hot: fx.hot
        property color ice: fx.ice
        property color tint: fx.here
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_synth_scene.frag.qsb"
    }
    ShaderEffectSource {
        id: worldTex
        sourceItem: world
        hideSource: true
        live: fx.running
        mipmap: true
        smooth: true
    }

    // The sun's rays, at a quarter of the size.
    ShaderEffect {
        id: shafts
        anchors.fill: parent
        blending: false
        property real aspect: width / Math.max(1, height)
        property point sun: fx.sun
        property var scene: worldTex
        fragmentShader: "qrc:/shaders/viz_synth_rays.frag.qsb"
    }
    ShaderEffectSource {
        id: raysTex
        sourceItem: shafts
        hideSource: true
        live: fx.running
        smooth: true
        textureSize: Qt.size(Math.max(2, Math.round(fx.width / 4)), Math.max(2, Math.round(fx.height / 4)))
    }

    ShaderEffect {
        anchors.fill: parent
        property real time: fx.clock
        property real aspect: width / Math.max(1, height)
        property real lines: height
        property real flare: fx.flare
        property real level: fx.level
        property real glitch: fx.glitch
        property point sun: fx.sun
        property color hot: fx.hot
        property color ice: fx.ice
        property var scene: worldTex
        property var rays: raysTex
        fragmentShader: "qrc:/shaders/viz_synth.frag.qsb"
    }
}
