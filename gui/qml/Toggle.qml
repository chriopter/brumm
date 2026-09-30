// Toggle is a switch: a pill lit in the color of what plays when on, its
// knob sliding across.
import QtQuick

Rectangle {
    id: t
    property bool on: false
    width: ui.px(38)
    height: ui.px(21)
    radius: height / 2
    color: on ? ui.here : Qt.alpha(ui.deep, 0.7)
    border { width: 1; color: on ? Qt.lighter(ui.here, 1.2) : ui.edge }
    Behavior on color { enabled: !ui.calm; ColorAnimation { duration: 150 } }
    Rectangle {
        width: parent.height - ui.px(6)
        height: width
        radius: width / 2
        y: ui.px(3)
        x: t.on ? parent.width - width - ui.px(3) : ui.px(3)
        color: ui.bright
        Behavior on x { enabled: !ui.calm; NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }
    }
}
