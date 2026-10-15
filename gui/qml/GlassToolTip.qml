import QtQuick
import QtQuick.Controls

ToolTip {
    id: tip
    delay: 400
    padding: ui.px(10)
    contentItem: Text {
        text: tip.text
        color: ui.bright
        font { family: ui.sans; pixelSize: ui.px(12) }
    }
    background: Rectangle {
        radius: ui.px(8)
        color: Qt.alpha(ui.deep, 0.96)
        border.color: Qt.alpha(ui.bright, 0.18)
    }
}
