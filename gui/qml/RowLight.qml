// RowLight is how the XMB lights what is chosen: a glow behind the row,
// brightest near the middle and falling off to nothing in every
// direction (shaders/glow.frag). It spills a little past the row it
// lights; drawn once, again only when it changes.
import QtQuick

Item {
    id: light
    property color color: ui.here
    property real strength: 1 // 1 chosen, less for a faint one
    property real center: 0.35 // where its middle is, across the row

    ShaderEffect {
        visible: ui.effects && light.strength > 0
        anchors { fill: parent; topMargin: -ui.px(14); bottomMargin: -ui.px(14); leftMargin: -ui.px(24); rightMargin: -ui.px(8) }
        property color tint: light.color
        property real strength: 0.42 * light.strength
        property real cx: light.center
        fragmentShader: "qrc:/shaders/glow.frag.qsb"
    }
    Rectangle { // without effects, a plain wash
        visible: !ui.effects && light.strength > 0
        anchors.fill: parent
        radius: ui.px(8)
        color: Qt.alpha(light.color, 0.16 * light.strength)
    }
}
