import QtQuick
import QtQuick.Controls

MenuItem {
    id: action
    implicitHeight: ui.px(34)
    height: visible ? implicitHeight : 0
    padding: ui.px(10)
    font { family: ui.sans; pixelSize: ui.px(13) }
    contentItem: Text {
        text: action.text
        color: !action.enabled ? ui.dim : action.highlighted ? ui.bright : ui.fg
        font: action.font
        verticalAlignment: Text.AlignVCenter
    }
    background: Item {
        RowLight { anchors.fill: parent; visible: action.highlighted; strength: 0.35 }
    }
}
