// Wire: vector-display 3D. A wireframe spins in perspective as neon light
// on a phosphor that fades in a few frames, so fast turns smear into
// motion blur, and stands mirrored on a glossy floor. Three shapes take
// turns (every 16 kicks or 24 seconds): an icosahedron whose vertices
// spike out with their spectrum band around a counter-rotating inner one,
// a tesseract turning through the fourth dimension, and a torus whose
// tube swells with the spectrum around its ring. Near edges burn white,
// far ones fade into the beam's color; the bass pumps the size, every
// kick throws in a spin that eases off, every fourth recolors the beam.
// The lines are projected once a frame and stroked (a Shape); then two
// passes: the phosphor at half size (shaders/viz_wire_trail.frag) and the
// screen, bloomed from its mip levels (shaders/viz_wire.frag).
import QtQuick
import QtQuick.Shapes

VizEffect {
    id: fx

    // the spin, carried from frame to frame
    readonly property var st: ({ ax: 0, ay: 0, az: 0, aw: 0, spin: 0, k: -1 })
    property real dt: 0
    readonly property real lw: Math.max(1.5, Math.min(width, height) / 420)

    // the beam's colors, as the terminal's three ramps: deep, lit
    readonly property var ramps: [[colors.blue, colors.cyan], [colors.magenta, colors.red], [colors.green, colors.yellow]]
    readonly property var ramp: ramps[Math.floor(audio.kicks / 4) % 3]

    // a theme color at full light
    function lit(c, s) {
        const q = Qt.color(c || fx.accent)
        return Qt.hsva(q.hsvHue < 0 ? 0 : q.hsvHue, Math.min(1, q.hsvSaturation * s + 0.15), 1, 1)
    }

    // The shapes' corners and edges, made once.
    readonly property var geo: {
        const phi = (1 + Math.sqrt(5)) / 2, n = Math.sqrt(1 + phi * phi)
        const ico = [], icoE = [], tesE = []
        for (const s1 of [-1, 1])
            for (const s2 of [-1, 1])
                ico.push([0, s1 / n, s2 * phi / n], [s1 / n, s2 * phi / n, 0], [s2 * phi / n, 0, s1 / n])
        for (let i = 0; i < 12; i++)
            for (let j = i + 1; j < 12; j++) {
                const d = Math.hypot(ico[i][0] - ico[j][0], ico[i][1] - ico[j][1], ico[i][2] - ico[j][2])
                if (Math.abs(d - 2 / n) < 1e-6) icoE.push([i, j])
            }
        for (let i = 0; i < 16; i++)
            for (let b = 0; b < 4; b++) {
                const j = i ^ (1 << b)
                if (j > i) tesE.push([i, j])
            }
        return { ico: ico, icoE: icoE, tesE: tesE }
    }
    readonly property int ringN: 20 // torus segments around the ring
    readonly property int tubeN: 8  // around the tube
    readonly property var px: new Array(160).fill(0)
    readonly property var py: new Array(160).fill(0)
    readonly property var pz: new Array(160).fill(0)

    // the spectrum at f (0–1), read between bands
    function at(sm, f) {
        const x = Math.max(0, Math.min(1, f)) * (sm.length - 1), i = Math.floor(x)
        return i + 1 < sm.length ? sm[i] + (sm[i + 1] - sm[i]) * (x - i) : sm[i]
    }

    // step turns the shape on by dt seconds and strokes it anew.
    function step(dt) {
        const a = fx.audio, s = fx.st, g = fx.geo
        fx.dt = dt
        if (s.k < 0) s.k = a.kicks
        if (a.kicks !== s.k) {
            s.k = a.kicks
            s.spin = Math.min(s.spin + 0.9, 2)
        }
        s.spin *= Math.exp(-dt * 3)
        const sp = 0.3 + a.live * 0.6 * a.level + s.spin
        s.ay += dt * sp
        s.ax += dt * sp * 0.61
        s.az += dt * sp * 0.23
        s.aw += dt * (0.3 + 0.6 * a.mid + 0.4 * s.spin)

        // rotation matrix Rz·Rx·Ry
        const cx = Math.cos(s.ax), sx = Math.sin(s.ax), cy = Math.cos(s.ay), sy = Math.sin(s.ay)
        const cz = Math.cos(s.az), sz = Math.sin(s.az)
        const m = [cz * cy - sz * sx * sy, -sz * cx, cz * sy + sz * sx * cy,
                   sz * cy + cz * sx * sy, cz * cx, sz * sy - cz * sx * cy,
                   -cx * sy, sx, cx * cy]
        const ox = width / 2, oy = height * 0.43
        const S = Math.min(width, height) * 0.28 * (1 + 0.16 * a.bass + 0.1 * a.beat)
        const X = fx.px, Y = fx.py, Z = fx.pz
        function put(i, x, y, z, inv) {
            let u, v, w
            if (inv) { // the transpose: the opposite turn
                u = m[0] * x + m[3] * y + m[6] * z
                v = m[1] * x + m[4] * y + m[7] * z
                w = m[2] * x + m[5] * y + m[8] * z
            } else {
                u = m[0] * x + m[1] * y + m[2] * z
                v = m[3] * x + m[4] * y + m[5] * z
                w = m[6] * x + m[7] * y + m[8] * z
            }
            const f = 3.2 / (w + 3.2 + 0.001) * S
            X[i] = ox + u * f
            Y[i] = oy + v * f
            Z[i] = Math.max(0, Math.min(1, (w + 1.4) / 2.8))
        }
        // edges go to one of three strokes by how lit they are
        const lines = [[], [], []], sparks = []
        function edge(i, j, bright) {
            const b = bright * (1 - 0.72 * (Z[i] + Z[j]) / 2)
            lines[b > 0.68 ? 0 : b > 0.44 ? 1 : 2].push([Qt.point(X[i], Y[i]), Qt.point(X[j], Y[j])])
        }
        // a vertex glints as a small cross, longer with the highs
        const glint = S * 0.07 * (0.4 + 0.6 * Math.min(1, a.high * 2.5)) * (1 + a.beat)
        function spark(i) {
            const r = glint * (1 - 0.6 * Z[i])
            sparks.push([Qt.point(X[i] - r, Y[i]), Qt.point(X[i] + r, Y[i])],
                        [Qt.point(X[i], Y[i] - r), Qt.point(X[i], Y[i] + r)])
        }

        const sm = a.sm
        switch ((Math.floor(a.t / 24) + Math.floor(a.kicks / 16)) % 3) {
        case 0: // icosahedron with spectrum spikes around a smaller, opposite one
            for (let i = 0; i < 12; i++) {
                const p = g.ico[i], r = 1 + 0.4 * fx.at(sm, i / 11)
                put(i, p[0] * r, p[1] * r, p[2] * r, false)
                put(12 + i, p[0] * 0.42, p[1] * 0.42, p[2] * 0.42, true)
            }
            for (const e of g.icoE) {
                edge(e[0], e[1], 1)
                edge(12 + e[0], 12 + e[1], 0.75)
            }
            for (let i = 0; i < 12; i++) spark(i)
            break
        case 1: { // tesseract: turn in the xw and yw planes, project 4D → 3D
            const c1 = Math.cos(s.aw), s1 = Math.sin(s.aw), c2 = Math.cos(s.aw * 0.7), s2 = Math.sin(s.aw * 0.7)
            const inner = 1 + 0.35 * a.bass
            for (let i = 0; i < 16; i++) {
                const p = [i & 1 ? 1 : -1, i & 2 ? 1 : -1, i & 4 ? 1 : -1, (i & 8 ? 1 : -1) * inner]
                const x = p[0] * c1 - p[3] * s1, w = p[0] * s1 + p[3] * c1
                const y = p[1] * c2 - w * s2, w2 = p[1] * s2 + w * c2
                const k = 0.55 * 2.4 / (2.4 - w2 * 0.6)
                put(i, x * k, y * k, p[2] * k, false)
            }
            for (const e of g.tesE) edge(e[0], e[1], 1)
            for (let i = 0; i < 16; i++) spark(i)
            break
        }
        default: { // torus, the tube swelling with the spectrum around the ring
            const M = fx.ringN, T = fx.tubeN
            for (let i = 0; i < M; i++) {
                const u = i / M * 2 * Math.PI
                const f = Math.abs(i / M * 2 - 1) // mirrored: bass at one side
                const tube = 0.3 * (1 + 1.1 * fx.at(sm, 1 - f))
                const cu = Math.cos(u), su = Math.sin(u)
                for (let j = 0; j < T; j++) {
                    const t = j / T * 2 * Math.PI + s.aw
                    const r = 0.82 + tube * Math.cos(t)
                    put(i * T + j, r * cu, r * su, tube * Math.sin(t), false)
                }
            }
            for (let i = 0; i < M; i++)
                for (let j = 0; j < T; j++) {
                    const k = i * T + j
                    edge(k, i * T + (j + 1) % T, 0.8)
                    edge(k, ((i + 1) % M) * T + j, 1)
                }
        }
        }
        nearLines.paths = lines[0]
        midLines.paths = lines[1]
        farLines.paths = lines[2]
        sparkLines.paths = sparks
    }

    FrameAnimation {
        running: fx.running
        onTriggered: fx.step(Math.min(frameTime, 0.1))
    }
    Component.onCompleted: step(0)

    // The lines as they stand this frame, white by how lit: drawn only
    // into their texture.
    Shape {
        id: shape
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        component Stroke: ShapePath {
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
        }
        Stroke { strokeColor: "#606060"; strokeWidth: fx.lw * 0.9; PathMultiline { id: farLines } }
        Stroke { strokeColor: "#a8a8a8"; strokeWidth: fx.lw * 1.2; PathMultiline { id: midLines } }
        Stroke { strokeColor: "#ffffff"; strokeWidth: fx.lw * 1.6; PathMultiline { id: nearLines } }
        Stroke { strokeColor: "#ffffff"; strokeWidth: fx.lw * 0.8; PathMultiline { id: sparkLines } }
    }
    ShaderEffectSource {
        id: linesTex
        sourceItem: shape
        hideSource: true
        live: fx.running
        smooth: true
    }

    // The phosphor: last frame's light, fading, under this frame's lines.
    ShaderEffect {
        id: trail
        anchors.fill: parent
        property real fade: Math.exp(-fx.dt * 14)
        property var prev: phosphor
        property var lines: linesTex
        fragmentShader: "qrc:/shaders/viz_wire_trail.frag.qsb"
    }
    ShaderEffectSource {
        id: phosphor
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
        property real aspect: width / Math.max(1, height)
        property real beat: fx.audio.beat
        property real bass: fx.audio.bass
        property real floorY: 0.8
        property color ground: theme.dark ? (fx.colors.darker_background || "#0e0e14") : "#0c0c14"
        property color deep: fx.lit(fx.ramp[0], 1.3)
        property color hot: fx.lit(fx.ramp[1], 1.1)
        property color halo: Qt.tint(fx.lit(fx.here, 1.2), Qt.alpha(deep, 0.5))
        property var lines: linesTex
        property var trail: phosphor
        fragmentShader: "qrc:/shaders/viz_wire.frag.qsb"
        Behavior on deep { ColorAnimation { duration: 600 } }
        Behavior on hot { ColorAnimation { duration: 600 } }
    }
}
