// Beam is a line of light: a crisp core and its glow, fading out smoothly
// toward both ends (shaders/beam.frag). It is laid along the middle of its
// height, which the glow spills into.
import QtQuick

Item {
    id: beam
    property color color: ui.here
    property real strength: 1
    property real glow: ui.px(6)
    height: 1

    ShaderEffect {
        visible: ui.effects
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
        height: Math.ceil(beam.glow * 3) * 2 + 1
        property color tint: beam.color
        property real strength: beam.strength
        property real h: height
        property real glow: beam.glow
        fragmentShader: "qrc:/shaders/beam.frag.qsb"
    }
    Rectangle { // without effects, the line alone
        visible: !ui.effects
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.5; color: Qt.alpha(beam.color, 0.7 * beam.strength) }
            GradientStop { position: 1; color: "transparent" }
        }
    }
}
