// UpdateBox is U: is there a newer brumm, install it, restart into it.
import QtQuick

Box {
    title: "update"
    readonly property var u: store.upd || {}

    Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        visible: !!(u.checking || u.installing)
        text: u.checking ? "Looking for a new release…" : "Installing " + (u.latest || "") + "…"
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(14) }
    }
    Text {
        visible: !u.checking && !u.installing
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        text: u.installed ? "✓  " + (u.latest || "The update") + " is installed"
            : u.err ? u.err
            : u.newer ? u.latest + " is available"
            : "✓  " + (u.current || "brumm") + " is the newest version"
        color: u.err ? ui.heart : ui.bright
        font { family: ui.sans; pixelSize: ui.px(16); weight: u.err ? Font.Normal : Font.DemiBold }
    }
    Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        visible: !!(u.installed || (u.newer && !u.installing))
        text: u.installed ? "The music goes on where it is." : "You have " + (u.current || "")
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(12) }
    }
    Column { // what the newer release says is new
        visible: !!u.newer && !u.installing && !u.installed && !!u.notes && u.notes.length > 0
        width: Math.min(parent.width, ui.px(460))
        anchors.horizontalCenter: parent.horizontalCenter
        topPadding: ui.px(10)
        spacing: ui.px(6)
        Repeater {
            model: (u.notes || []).slice(0, 12)
            Text {
                required property string modelData
                required property int index
                width: parent.width
                wrapMode: Text.WordWrap
                readonly property bool head: modelData.startsWith("# ") // a section: New, Fixed
                topPadding: head && index > 0 ? ui.px(6) : 0
                text: head ? modelData.slice(2) : modelData
                color: head ? ui.bright : ui.fg
                font { family: ui.sans; pixelSize: ui.px(12); weight: head ? Font.DemiBold : Font.Normal }
            }
        }
    }
    Item { width: 1; height: ui.px(8) }
    KeyHints {
        visible: !u.checking && !u.installing
        pairs: u.installed ? [["enter", "restart now"], ["esc", "at the next pause"]]
             : u.newer ? [["enter", "install"], ["esc", "later"]] : [["esc", "close"]]
    }
}
