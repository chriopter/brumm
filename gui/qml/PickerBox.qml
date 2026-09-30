// PickerBox is P: the playlist a song, album or playlist goes to.
import QtQuick

Box {
    id: pb
    title: store.pick ? "add “" + store.pick.name + "” to" : ""
    readonly property var p: store.pick || { lists: [], sel: 0 }
    readonly property int shown: 9
    readonly property int first: Math.max(0, Math.min(p.sel - shown + 1, p.lists.length + 1 - shown))

    Text {
        visible: pb.p.loading === true
        text: "Loading your playlists…"
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(14) }
    }
    Repeater {
        model: pb.p.loading ? [] : [{ name: "New playlist…", fresh: true }].concat(pb.p.lists).slice(pb.first, pb.first + pb.shown)
        Item {
            required property int index
            required property var modelData
            readonly property int at: pb.first + index
            readonly property bool picked: pb.p.sel === at
            width: parent.width
            height: ui.px(36)
            RowLight {
                anchors { fill: parent; leftMargin: -ui.px(8); rightMargin: -ui.px(8) }
                visible: parent.picked
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                elide: Text.ElideRight
                text: modelData.fresh && pb.p.naming ? "Name: " + pb.p.input + "▏" : modelData.name
                color: modelData.fresh ? ui.here : parent.picked ? ui.bright : ui.fg
                font { family: ui.sans; pixelSize: ui.px(14); weight: modelData.fresh ? Font.DemiBold : Font.Normal }
            }
            TapHandler { onTapped: store.pickRow(parent.at) }
        }
    }
    Item { width: 1; height: ui.px(8) }
    KeyHints { pairs: pb.p.naming ? [["enter", "make it"], ["esc", "back"]] : [["↑↓", "move"], ["enter", "add"], ["esc", "close"]] }
}
