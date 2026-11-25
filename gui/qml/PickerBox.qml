// PickerBox is P: the playlist a song, album or playlist goes to.
import QtQuick
import QtQuick.Controls

Box {
    id: pb
    title: "Add to playlist"
    readonly property var p: store.pick || { lists: [], sel: 0 }
    readonly property int rowHeight: ui.px(36)

    Text {
        width: parent.width
        text: pb.p.name || ""
        elide: Text.ElideRight
        color: ui.bright
        font { family: ui.sans; pixelSize: ui.px(16); weight: Font.DemiBold }
    }
    Text {
        visible: pb.p.loading === true
        text: "Loading your playlists…"
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(14) }
    }
    Rectangle {
        width: parent.width
        height: pb.rowHeight
        visible: !pb.p.loading
        radius: ui.px(8)
        color: Qt.alpha(ui.here, pb.p.sel === 0 || createHover.hovered ? 0.16 : 0.06)
        Text {
            anchors { fill: parent; margins: ui.px(8) }
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            text: pb.p.naming ? "Name: " + pb.p.input + "▏" : "+  New playlist…"
            color: ui.here
            font { family: ui.sans; pixelSize: ui.px(14); weight: Font.DemiBold }
        }
        HoverHandler { id: createHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: if (!pb.p.naming) store.pickRow(0) }
    }
    ListView {
        id: lists
        objectName: "playlistChoices"
        width: parent.width
        height: Math.min(count * pb.rowHeight, 8 * pb.rowHeight, Math.max(pb.rowHeight, pb.parent.height - ui.px(250)))
        visible: count > 0 && !pb.p.naming
        model: pb.p.lists
        currentIndex: pb.p.sel - 1
        keyNavigationEnabled: false
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: lists.contentHeight > lists.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
        delegate: Rectangle {
            required property int index
            required property var modelData
            width: lists.width - ui.px(12)
            height: pb.rowHeight
            radius: ui.px(6)
            color: Qt.alpha(ui.here, ListView.isCurrentItem ? 0.18 : hover.hovered ? 0.09 : 0)
            Text {
                anchors { fill: parent; leftMargin: ui.px(8); rightMargin: ui.px(8) }
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                text: modelData.name
                color: parent.ListView.isCurrentItem ? ui.bright : ui.fg
                font { family: ui.sans; pixelSize: ui.px(14) }
            }
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: store.pickRow(parent.index + 1) }
        }
    }
    KeyHints { pairs: pb.p.naming ? [["enter", "make it"], ["esc", "back"]] : [["↑↓", "move"], ["enter", "add"], ["esc", "close"]] }
}
