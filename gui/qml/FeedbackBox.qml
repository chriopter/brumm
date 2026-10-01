// FeedbackBox is !: write what is wrong or wished for, then send it to
// Omarchy's agent, which looks into it and files the issue, or open the
// issue on GitHub yourself. enter does the first there is.
import QtQuick

Box {
    id: fbox
    title: "send feedback or report an issue"
    boxWidth: ui.px(560)
    readonly property var f: store.fb || { input: "", sending: false }

    Rectangle {
        width: parent.width
        height: Math.max(ui.px(96), typed.implicitHeight + ui.px(20))
        radius: ui.px(10)
        color: Qt.alpha(ui.deep, 0.6)
        border { width: 1; color: Qt.alpha(ui.here, 0.6) }
        Text {
            id: typed
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: ui.px(10) }
            text: fbox.f.input + (fbox.f.sending ? "" : "▏")
            color: ui.bright
            wrapMode: Text.Wrap
            font { family: ui.sans; pixelSize: ui.px(14) }
        }
    }
    Item { width: 1; height: ui.px(8) }

    // The two ways to send it; the first is what enter does.
    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: ui.px(12)
        opacity: fbox.f.sending ? 0.4 : 1
        Repeater {
            model: daemon.agent ? [["󰚩", "Send to my agent", true], ["󰊤", "Open on GitHub", false]] : [["󰊤", "Open on GitHub", false]]
            Item {
                id: b
                required property int index
                required property var modelData
                readonly property bool main: index === 0
                width: label.implicitWidth + ui.px(34)
                height: ui.px(36)
                GlassShape { anchors.fill: parent; visible: ui.effects; lit: b.main ? 1 : 0; hot: hover.hovered ? 1 : 0 }
                Rectangle { anchors.fill: parent; visible: !ui.effects; radius: height / 2; color: b.main ? ui.here : Qt.alpha(ui.bright, 0.1) }
                Row {
                    id: label
                    anchors.centerIn: parent
                    spacing: ui.px(8)
                    Text { anchors.verticalCenter: parent.verticalCenter; text: b.modelData[0]; color: b.main ? ui.deep : ui.fg; font { family: ui.mono; pixelSize: ui.px(15) } }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: b.modelData[1]; color: b.main ? ui.deep : ui.fg; font { family: ui.sans; pixelSize: ui.px(13); weight: Font.DemiBold } }
                }
                HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: store.sendFeedback(b.modelData[2]) }
            }
        }
    }
    Item { width: 1; height: ui.px(2) }
    Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: fbox.f.sending ? "Opening…" : "enter sends · esc cancels · it becomes a public issue on GitHub"
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(11) }
    }
}
