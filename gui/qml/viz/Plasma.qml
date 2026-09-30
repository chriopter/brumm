// Plasma: a demoscene plasma, four interfering sine fields indexing a
// copper ring of theme colors — dark, blue, cyan, white, gold, red,
// magenta and back — drawn as lit molten glass. Loudness drives the flow,
// the bass deepens the rings from the middle, the mids turn the colors,
// and every kick sends a ripple outward. One pass (shaders/viz_plasma.frag);
// the ring of colors is a small gradient texture, drawn again only when the
// theme changes.
import QtQuick

VizEffect {
    id: fx

    property real flow: 0
    property real speed: 0.3
    property real hue: 0
    property real bass: 0
    property int kicks: -1
    property var starts: [-1, -1, -1, -1] // the ripples' start times
    property vector4d ripples: Qt.vector4d(-1, -1, -1, -1)

    FrameAnimation {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.speed += (0.3 + a.live * 1.4 * a.level - fx.speed) * Math.min(1, dt * 1.2)
            fx.flow = (fx.flow + dt * fx.speed) % 6283.1853
            fx.hue = (fx.hue + dt * (0.25 + 0.9 * a.mid) / 10) % 1
            fx.bass += (a.bass - fx.bass) * Math.min(1, dt * 5)
            if (a.kicks !== fx.kicks) {
                if (fx.kicks >= 0) fx.starts = [a.t].concat(fx.starts.slice(0, 3))
                fx.kicks = a.kicks
            }
            const s = fx.starts, age = i => s[i] < 0 || a.t - s[i] > 1.6 ? -1 : a.t - s[i]
            fx.ripples = Qt.vector4d(age(0), age(1), age(2), age(3))
        }
    }

    // a theme color, lit: fuller and brighter
    function lit(c, v) {
        const q = Qt.color(c)
        return Qt.hsva(q.hsvHue < 0 ? 0 : q.hsvHue, Math.min(1, q.hsvSaturation * 1.2), v, 1)
    }
    readonly property var ring: [
        Qt.tint(colors.darker_background || "#0e0e14", Qt.alpha(here, 0.15)),
        lit(colors.blue, 0.8), lit(colors.cyan, 0.85), lit(colors.cyan, 1),
        Qt.tint(colors.bright_foreground || "#ffffff", "#80ffffff"), lit(colors.yellow, 1),
        lit(colors.yellow, 0.85), lit(colors.red, 0.95), lit(colors.magenta, 0.9), lit(colors.blue, 0.7)]

    // the ring of colors, left to right and back to the first
    Rectangle {
        id: strip
        width: 256
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: fx.ring[0] }
            GradientStop { position: 0.1; color: fx.ring[1] }
            GradientStop { position: 0.2; color: fx.ring[2] }
            GradientStop { position: 0.3; color: fx.ring[3] }
            GradientStop { position: 0.4; color: fx.ring[4] }
            GradientStop { position: 0.5; color: fx.ring[5] }
            GradientStop { position: 0.6; color: fx.ring[6] }
            GradientStop { position: 0.7; color: fx.ring[7] }
            GradientStop { position: 0.8; color: fx.ring[8] }
            GradientStop { position: 0.9; color: fx.ring[9] }
            GradientStop { position: 1.0; color: fx.ring[0] }
        }
    }
    ShaderEffectSource {
        id: palette
        sourceItem: strip
        hideSource: true
        smooth: true
        wrapMode: ShaderEffectSource.Repeat
    }

    ShaderEffect {
        anchors.fill: parent
        property real time: fx.flow
        property real aspect: width / Math.max(1, height)
        property real hue: fx.hue
        property real bass: fx.bass
        property real beat: fx.audio.beat
        property vector4d ripples: fx.ripples
        property var ring: palette
        fragmentShader: "qrc:/shaders/viz_plasma.frag.qsb"
    }
}
