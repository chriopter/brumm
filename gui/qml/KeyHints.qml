// KeyHints close a popup: a hairline fading out at both ends, and under
// it, centered, the keys that answer it, each with what it does.
import QtQuick

Column {
    id: hints
    property var pairs: []
    width: parent ? parent.width : implicitWidth
    spacing: ui.px(12)

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.5; color: Qt.alpha(ui.bright, 0.16) }
            GradientStop { position: 1; color: "transparent" }
        }
    }
    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: ui.px(20)
        Repeater {
            model: hints.pairs
            Row {
                required property var modelData
                spacing: ui.px(7)
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(height, cap.implicitWidth + ui.px(12))
                    height: ui.px(20)
                    radius: ui.px(6)
                    color: Qt.alpha(ui.bright, 0.09)
                    Text {
                        id: cap
                        anchors.centerIn: parent
                        text: modelData[0]
                        color: ui.bright
                        font { family: ui.sans; pixelSize: ui.px(11); weight: Font.DemiBold }
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData[1]
                    color: ui.dim
                    font { family: ui.sans; pixelSize: ui.px(12) }
                }
            }
        }
    }
}
