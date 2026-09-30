// Footer is what just happened, said for a moment at the bottom right.
import QtQuick

Item {
    height: ui.px(22)
    Text {
        anchors { right: parent.right; rightMargin: ui.px(4); verticalCenter: parent.verticalCenter }
        text: store.flash || (daemon.connected ? "" : "connecting to brumm…")
        color: ui.fg
        opacity: text ? 1 : 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 200 } }
        font { family: ui.sans; pixelSize: ui.px(12) }
    }
}
