import QtQuick

Box {
    id: services
    title: "Music services"
    scrollable: true
    boxWidth: ui.px(470)

    Text {
        width: parent.width
        text: store.setupMode ? "Add your music services" : "Your music services"
        color: ui.bright
        font { family: ui.sans; pixelSize: ui.px(25); weight: Font.DemiBold }
    }
    Text {
        width: parent.width
        text: "Connect your services. You can switch services or add another at any time."
        wrapMode: Text.WordWrap
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(13) }
    }
    Item { width: 1; height: ui.px(12) }
    Repeater {
        model: store.providers
        Rectangle {
            id: service
            required property var modelData
            required property int index
            readonly property bool active: store.selectedProvider === modelData.id
            readonly property bool busy: store.connectingProvider === modelData.id || (active && !!store.st.auth)
            readonly property bool expired: active && store.st.status === "logged-out" && !busy
            width: parent.width
            height: ui.px(busy && active && store.st.auth ? 148 : 82)
            radius: ui.px(12)
            color: Qt.alpha(ui.bright, 0.035)
            border { color: store.providerIndex === index || (active && modelData.configured) ? Qt.alpha(ui.here, 0.5) : ui.edge; width: 1 }

            Text {
                anchors { left: parent.left; leftMargin: ui.px(18); top: parent.top; topMargin: ui.px(14) }
                text: service.modelData.name
                color: ui.bright
                font { family: ui.sans; pixelSize: ui.px(17); weight: Font.DemiBold }
            }
            Text {
                anchors { left: parent.left; leftMargin: ui.px(18); top: parent.top; topMargin: ui.px(43) }
                text: service.busy ? "Complete sign-in in your browser" : service.expired ? "Sign in again to listen" : service.modelData.configured ? (service.active ? "Connected · Active" : "Connected") : "Not connected"
                color: service.modelData.configured ? ui.here : ui.dim
                font { family: ui.sans; pixelSize: ui.px(12) }
            }
            Rectangle {
                anchors { right: parent.right; rightMargin: ui.px(16); top: parent.top; topMargin: ui.px(23) }
                width: ui.px(92); height: ui.px(34); radius: height / 2
                color: actionHover.hovered ? Qt.alpha(ui.here, 0.25) : Qt.alpha(ui.here, 0.12)
                border.color: Qt.alpha(ui.here, 0.4)
                readonly property bool allowed: service.modelData.available && (!store.connectingProvider || service.busy)
                opacity: allowed ? 1 : 0.4
                Text {
                    anchors.centerIn: parent
                    text: service.busy ? "Cancel" : service.expired ? "Sign in" : service.modelData.configured ? (service.active ? "Active" : "Switch") : "Connect"
                    color: ui.bright
                    font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold }
                }
                HoverHandler { id: actionHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    enabled: parent.allowed
                    onTapped: {
                        store.providerIndex = service.index
                        if (service.busy) store.cancelConnection()
                        else if (!service.modelData.configured || service.expired) store.connectProvider(service.modelData.id)
                        else store.chooseProvider(service.modelData.id, false)
                    }
                }
            }
            Text {
                anchors { left: parent.left; right: parent.right; leftMargin: ui.px(18); rightMargin: ui.px(18); top: parent.top; topMargin: ui.px(78) }
                visible: service.busy && service.active && !!store.st.auth
                text: store.st.auth ? (store.st.auth.code ? "Code: " + store.st.auth.code + "\n" : "") + store.st.auth.url : ""
                wrapMode: Text.WrapAnywhere
                color: ui.here
                font { family: ui.sans; pixelSize: ui.px(13) }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: if (store.st.auth) Qt.openUrlExternally(store.st.auth.url) }
            }
        }
    }
    Text {
        width: parent.width
        text: store.flash || (store.st.err ? store.st.err : "")
        visible: text !== ""
        wrapMode: Text.WordWrap
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(12) }
    }
    Item { width: 1; height: ui.px(10) }
    Rectangle {
        anchors.right: parent.right
        width: ui.px(116); height: ui.px(38); radius: height / 2
        opacity: store.connectedProviders.length && !store.connectingProvider ? 1 : 0.35
        color: ui.here
        Text { anchors.centerIn: parent; text: store.setupMode ? "Continue" : "Done"; color: ui.deep; font { family: ui.sans; pixelSize: ui.px(13); weight: Font.DemiBold } }
        HoverHandler { cursorShape: store.connectedProviders.length ? Qt.PointingHandCursor : Qt.ArrowCursor }
        TapHandler { enabled: store.connectedProviders.length > 0 && !store.connectingProvider; onTapped: store.finishServices() }
    }
}
