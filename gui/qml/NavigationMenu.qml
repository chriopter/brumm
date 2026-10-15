import QtQuick
import QtQuick.Controls

Item {
    id: nav
    width: ui.px(34)
    height: width
    signal activated()
    Orb {
        size: nav.width
        icon: "󰅀"
        tooltip: "Browse music"
        on: destinations.opened
        onClicked: { store.closeMenu(); destinations.open() }
    }
    Menu {
        id: destinations
        y: nav.height + ui.px(8)
        width: Math.min(ui.px(240), win.width - ui.px(24))
        height: Math.min(implicitHeight, win.height - nav.height - ui.px(40))
        padding: ui.px(8)
        background: Rectangle {
            radius: ui.px(12)
            color: Qt.alpha(ui.deep, 0.97)
            border.color: ui.edge
        }
        Repeater {
            model: store.navigation.slice(0, store.secQueue)
            MenuItem {
                required property int index
                required property var modelData
                text: store.sectionNames[index]
                height: ui.px(36)
                readonly property bool selected: store.section === index && !store.playerView
                contentItem: Text {
                    text: store.sectionIcons[parent.index] + "   " + parent.text
                    color: parent.selected ? ui.here : ui.bright
                    verticalAlignment: Text.AlignVCenter
                    font { family: ui.sans; pixelSize: ui.px(14) }
                }
                background: Rectangle { radius: ui.px(6); color: parent.highlighted ? ui.glassHi : "transparent" }
                onTriggered: {
                    store.switchTo(index)
                    store.compactBrowser = true
                    store.sideFocus = false
                    nav.activated()
                }
            }
        }
        MenuItem {
            text: "Now Playing"
            height: ui.px(36)
            contentItem: Text {
                text: "♪   " + parent.text
                color: store.playerView ? ui.here : ui.bright
                verticalAlignment: Text.AlignVCenter
                font { family: ui.sans; pixelSize: ui.px(14) }
            }
            background: Rectangle { radius: ui.px(6); color: parent.highlighted ? ui.glassHi : "transparent" }
            onTriggered: { store.showPlayer(); nav.activated() }
        }
        MenuSeparator {}
    }
}
