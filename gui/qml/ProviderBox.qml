// ProviderBox picks the music service: on the first start, and from the
// menu's Music Service. The one in use is marked; one still to come shows
// but cannot be picked.
import QtQuick

Box {
    id: pv
    title: "music service"

    Text {
        width: parent.width
        wrapMode: Text.WordWrap
        text: store.providerPicking ? "Where your music comes from. Your library and place stay with each one."
                                    : "Choose the service you use for music."
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(13) }
    }
    Item { width: 1; height: ui.px(6) }
    Text {
        visible: store.providers.length === 0
        text: "Loading the services…"
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(14) }
    }
    Repeater {
        model: store.providers
        Item {
            required property int index
            required property var modelData
            readonly property bool picked: store.providerIndex === index
            readonly property bool current: store.providerPicking && modelData.id === store.providerSelected
            width: parent.width
            height: ui.px(36)
            RowLight {
                anchors { fill: parent; leftMargin: -ui.px(8); rightMargin: -ui.px(8) }
                visible: parent.picked && modelData.available
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.name
                color: !modelData.available ? ui.dim : parent.picked ? ui.bright : ui.fg
                font { family: ui.sans; pixelSize: ui.px(14); weight: parent.current ? Font.DemiBold : Font.Normal }
            }
            Text {
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                text: parent.current ? "in use" : modelData.available ? "" : "coming soon"
                color: parent.current ? ui.here : ui.dim
                font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold }
            }
            TapHandler {
                enabled: modelData.available
                onTapped: { store.providerIndex = index; store.chooseProvider(modelData.id) }
            }
        }
    }
    Item { width: 1; height: ui.px(8) }
    KeyHints {
        pairs: store.providerPicking ? [["↑↓", "move"], ["enter", "switch"], ["esc", "close"]]
                                     : [["↑↓", "move"], ["enter", "choose"]]
    }
}
