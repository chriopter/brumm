// MorphButton is a quiet button: an icon and a word. Pointed at, it opens
// into a pill of light and shows the key that does the same.
import QtQuick

Item {
    id: m
    property string icon
    property string label
    property string key
    property bool quiet: false // an icon alone until pointed at
    property bool lit: false // on: its icon in the color of what plays
    signal clicked()

    readonly property bool open: hover.hovered
    implicitWidth: row.implicitWidth + ui.px(20)
    implicitHeight: ui.px(30)
    Behavior on implicitWidth { enabled: !ui.calm; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: Qt.alpha(ui.here, 0.16)
        border { width: 1; color: Qt.alpha(ui.here, 0.5) }
        opacity: m.open ? 1 : 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 180 } }
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: ui.px(7)
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: m.icon
            color: m.open || m.lit ? ui.here : ui.dim
            font { family: ui.mono; pixelSize: ui.px(14) }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: m.label
            visible: m.label !== "" && (!m.quiet || m.open) // quiet: the word only when pointed at
            color: m.open ? ui.bright : ui.dim
            font { family: ui.sans; pixelSize: ui.px(12) }
        }
        // The key, growing in from nothing.
        Item {
            anchors.verticalCenter: parent.verticalCenter
            width: m.open ? cap.width : 0
            height: cap.height
            clip: true
            Behavior on width { enabled: !ui.calm; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Rectangle {
                id: cap
                width: Math.max(height, capText.implicitWidth + ui.px(10))
                height: ui.px(18)
                radius: ui.px(5)
                color: Qt.alpha(ui.deep, 0.6)
                border { width: 1; color: Qt.alpha(ui.here, 0.6) }
                Text {
                    id: capText
                    anchors.centerIn: parent
                    text: m.key
                    color: ui.bright
                    font { family: ui.sans; pixelSize: ui.px(10); weight: Font.Bold }
                }
            }
        }
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: m.clicked() }
}
