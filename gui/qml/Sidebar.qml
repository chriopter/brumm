// Sidebar runs down the left edge: the menu on top, the sections under it
// as icons, the active one lit from behind with a glint under it. ← from the list comes here;
// ↑↓ go through the sections, → or enter back into the list.
import QtQuick
import QtQuick.Effects

Item {
    id: side
    width: ui.px(46)

    MenuButton {
        id: menu
        z: 10
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top }
    }

    Column {
        id: icons
        anchors { horizontalCenter: parent.horizontalCenter; top: menu.bottom; topMargin: ui.px(26) }
        spacing: ui.px(8)
        Repeater {
            model: store.sectionIcons
            Item {
                id: tab
                required property int index
                required property string modelData
                readonly property bool active: store.section === index
                width: ui.px(40)
                height: ui.px(38)
                RectangularShadow { // lit from behind, as the XMB lights what is chosen
                    visible: tab.active || hover.hovered
                    anchors { fill: parent; margins: ui.px(4) }
                    radius: ui.px(10)
                    blur: store.sideFocus && tab.active ? ui.px(22) : ui.px(16)
                    color: tab.active ? Qt.alpha(ui.here, store.sideFocus ? 0.8 : 0.5) : Qt.alpha(ui.bright, 0.08)
                    Behavior on blur { enabled: !ui.calm; NumberAnimation { duration: 150 } }
                }
                Item { // and a glint of light under it
                    visible: tab.active
                    anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: ui.px(4) }
                    width: ui.px(24)
                    height: 1
                    RectangularShadow {
                        visible: ui.effects
                        anchors { fill: parent; leftMargin: ui.px(5); rightMargin: ui.px(5) }
                        blur: ui.px(4)
                        color: Qt.alpha(ui.here, store.sideFocus ? 0.9 : 0.6)
                    }
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: "transparent" }
                            GradientStop { position: 0.5; color: Qt.tint(ui.here, Qt.alpha("white", store.sideFocus ? 0.7 : 0.45)) }
                            GradientStop { position: 1; color: "transparent" }
                        }
                    }
                }
                Text {
                    anchors.centerIn: parent
                    anchors.verticalCenterOffset: -ui.px(1)
                    text: tab.modelData
                    color: tab.active ? ui.bright : hover.hovered ? ui.fg : ui.dim
                    font { family: ui.mono; pixelSize: ui.px(19) }
                }
                HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: { store.switchTo(tab.index); store.sideFocus = false } }
            }
        }
    }
}
