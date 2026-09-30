// Controls are the transport, all glass: shuffle and repeat; back, the
// play orb, next; add, radio and the volume.
import QtQuick

Item {
    id: ctl
    height: play.height

    Row {
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        spacing: ui.px(10)
        Orb { size: ui.px(34); icon: "󰒝"; on: !!store.st.shuffle; onClicked: store.key("s") }
        Orb { size: ui.px(34); icon: store.st.repeat === 1 ? "󰑘" : "󰑖"; on: (store.st.repeat || 0) > 0; onClicked: store.key("r") }
    }

    Row {
        anchors.centerIn: parent
        spacing: ui.px(18)
        Orb { anchors.verticalCenter: parent.verticalCenter; size: ui.px(46); icon: "󰒮"; onClicked: store.key("p") }
        Orb { id: play; size: ui.px(66); lit: true; icon: store.st.playing ? "󰏤" : "󰐊"; onClicked: store.toggle() }
        Orb { anchors.verticalCenter: parent.verticalCenter; size: ui.px(46); icon: "󰒭"; onClicked: store.key("n") }
    }

    // Add, radio and the volume, glass as the rest.
    Row {
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: ui.px(10)
        visible: !store.st.preview
        Orb {
            id: add
            size: ui.px(34); icon: "󰐕"; on: store.addOpen
            onClicked: store.addOpen = !store.addOpen
        }
        Orb { size: ui.px(34); icon: "󰐹"; onClicked: store.radioPlaying() }
        // The volume: click mutes, the wheel turns it and says how loud.
        Orb {
            size: ui.px(34)
            icon: (store.st.volume || 0) > 0 ? "󰕾" : "󰖁"
            onClicked: store.key("m")
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.NoButton
                // Each notch turns it 5%; a touchpad's small steps add up.
                // Up is louder, however the scrolling is set to feel.
                property real acc: 0
                onWheel: w => {
                    const dy = w.inverted ? -w.angleDelta.y : w.angleDelta.y
                    acc += dy
                    while (Math.abs(acc) >= 120) {
                        store.setVolume((store.st.volume || 0) + (acc > 0 ? 0.05 : -0.05))
                        acc -= acc > 0 ? 120 : -120
                    }
                }
            }
        }
    }

    // What + offers: a card of glass over the button.
    Rectangle {
        id: addCard
        parent: win.contentItem
        visible: store.addOpen && ctl.visible // only the controls on show: the visualizer has its own
        z: 60
        readonly property point at: { store.addOpen; win.width; win.height; return add.mapToItem(win.contentItem, 0, 0) }
        width: ui.px(210)
        height: addRows.implicitHeight + ui.px(12)
        x: Math.min(at.x + add.width / 2 - width / 2, win.width - width - ui.px(8))
        y: at.y - height - ui.px(10)
        radius: ui.px(12)
        color: ui.tint(ui.deep, 0.08)
        border { width: 1; color: Qt.alpha(ui.bright, 0.14) }
        TapHandler {}
        Column {
            id: addRows
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: ui.px(6) }
            Repeater {
                model: [["󰲸", "Add to Playlist…", "P", () => store.pickPlaying()],
                        ["󰋚", "Add to Library", "i", () => store.libraryPlaying()]]
                Item {
                    required property var modelData
                    width: addRows.width
                    height: ui.px(34)
                    RowLight { anchors.fill: parent; visible: rowHover.hovered }
                    Text {
                        id: glyph
                        anchors { left: parent.left; leftMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        text: modelData[0]; color: rowHover.hovered ? ui.here : ui.dim
                        font { family: ui.mono; pixelSize: ui.px(15) }
                    }
                    Text {
                        anchors { left: glyph.right; leftMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        text: modelData[1]; color: rowHover.hovered ? ui.bright : ui.fg
                        font { family: ui.sans; pixelSize: ui.px(13) }
                    }
                    Text {
                        anchors { right: parent.right; rightMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        text: modelData[2]; color: ui.dim
                        font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold }
                    }
                    HoverHandler { id: rowHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: { store.addOpen = false; modelData[3]() } }
                }
            }
        }
    }
}
