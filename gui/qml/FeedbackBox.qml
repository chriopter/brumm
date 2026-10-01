// FeedbackBox is !: say what is wrong; enter opens a GitHub issue filled
// in with it, tab has Omarchy's agent look into it first.
import QtQuick

Box {
    title: "feedback"
    boxWidth: ui.px(560)
    readonly property var f: store.fb || { input: "", sending: false }

    Text {
        width: parent.width
        text: "What is wrong, or what would you like?"
        color: ui.bright
        font { family: ui.sans; pixelSize: ui.px(16); weight: Font.DemiBold }
    }
    Item { width: 1; height: ui.px(4) }
    Rectangle {
        width: parent.width
        height: Math.max(ui.px(84), typed.implicitHeight + ui.px(20))
        radius: ui.px(10)
        color: Qt.alpha(ui.deep, 0.6)
        border { width: 1; color: Qt.alpha(ui.here, 0.6) }
        Text {
            id: typed
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: ui.px(10) }
            text: f.input + (f.sending ? "" : "▏")
            color: ui.bright
            wrapMode: Text.Wrap
            font { family: ui.sans; pixelSize: ui.px(14) }
        }
    }
    Text {
        width: parent.width
        wrapMode: Text.WordWrap
        text: f.sending ? "Opening…" : "It becomes a public issue on github.com/chriopter/brumm, with your brumm version and system."
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(12) }
    }
    Item { width: 1; height: ui.px(4) }
    KeyHints { visible: !f.sending; pairs: (daemon.omarchy ? [["enter", "open on GitHub"], ["tab", "let my agent look into it"]] : [["enter", "open on GitHub"]]).concat([["esc", "cancel"]]) }
}
