import QtQuick
import QtQuick.Controls

Item {
    id: nav
    height: ui.px(44)
    property bool searchExpanded: false
    readonly property bool compact: width < ui.px(1050)
    FontMetrics { id: nameMetrics; font { family: ui.sans; pixelSize: ui.px(15); weight: Font.DemiBold } }
    FontMetrics { id: iconMetrics; font { family: ui.mono; pixelSize: ui.px(18) } }
    function namedWidth(index) { return nameMetrics.advanceWidth(store.sectionNames[index]) + iconMetrics.advanceWidth(store.sectionIcons[index]) + ui.px(31) }
    readonly property var namedTabs: {
        const count = store.secSearch
        const names = Array(count).fill(true)
        let needed = ui.px(56 + (count - 1) * 8) + nowPlaying.width + searchBox.width + ui.px(16)
        for (let i = 0; i < count; i++) needed += namedWidth(i)
        // Collapse from the right; keep the active section named last.
        const order = []
        for (let i = count - 1; i >= 0; i--) if (i !== store.section) order.push(i)
        if (store.section < count) order.push(store.section)
        for (const i of order) {
            if (needed <= viewport.width) break
            names[i] = false
            needed -= namedWidth(i) - ui.px(38)
        }
        return names
    }
    signal searchFinished()
    onSearchFinished: { searchExpanded = false; store.searching = false }
    Connections {
        target: store
        function onSearchingChanged() {
            if (store.searching) { nav.searchExpanded = true; searchInput.forceActiveFocus() }
        }
    }

    MenuButton {
        id: menu
        z: 2
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
    }

    Flickable {
        id: viewport
        // Leave room inside the clipped viewport for RowLight's spill.
        anchors { left: menu.right; leftMargin: -ui.px(2); right: parent.right; rightMargin: ui.px(8); top: parent.top; topMargin: -ui.px(14); bottom: parent.bottom; bottomMargin: -ui.px(14) }
        contentWidth: tabs.width + ui.px(32)
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        clip: true
        Row {
        id: tabs
        x: ui.px(24)
        y: ui.px(14)
        height: nav.height
        spacing: ui.px(8)
        Repeater {
            model: store.sectionNames.slice(0, store.secSearch)
            Item {
                id: tab
                required property int index
                required property string modelData
                readonly property bool active: !store.playerView && store.section === index
                width: nav.namedTabs[index] ? nav.namedWidth(index) : ui.px(38)
                height: tabs.height
                RowLight {
                    anchors.fill: parent
                    visible: tab.active || hover.hovered
                    color: tab.active ? ui.here : ui.bright
                    strength: tab.active ? 0.65 : 0.2
                }
                Row {
                    id: label
                    anchors.centerIn: parent
                    spacing: ui.px(7)
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: store.sectionIcons[tab.index] || ""
                        color: tab.active ? ui.bright : hover.hovered ? ui.fg : ui.dim
                        font { family: ui.mono; pixelSize: ui.px(18) }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: tab.modelData
                        visible: !!nav.namedTabs[tab.index]
                        color: tab.active ? ui.bright : hover.hovered ? ui.fg : ui.dim
                        font { family: ui.sans; pixelSize: ui.px(15); weight: tab.active ? Font.DemiBold : Font.Normal }
                    }
                }
                Beam {
                    visible: tab.active
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: ui.px(10) }
                }
                HoverHandler {
                    id: hover
                    cursorShape: Qt.PointingHandCursor
                }
                ToolTip.visible: !nav.namedTabs[tab.index] && hover.hovered
                ToolTip.text: tab.modelData
                ToolTip.delay: 350
                TapHandler { onTapped: { store.switchTo(tab.index); store.sideFocus = false; nav.searchFinished() } }
            }
        }
        Item {
            id: nowPlaying
            readonly property real namedWidth: nameMetrics.advanceWidth("Now Playing") + ui.px(39)
            readonly property bool named: viewport.width >= ui.px(56 + (store.secSearch - 1) * 8 + store.secSearch * 38 + 8) + namedWidth
            width: named ? namedWidth : ui.px(38)
            height: tabs.height
            RowLight { anchors.fill: parent; visible: store.playerView || nowHover.hovered; strength: store.playerView ? 0.65 : 0.2 }
            Row {
                anchors.centerIn: parent
                spacing: ui.px(8)
                Equalizer { anchors.verticalCenter: parent.verticalCenter; running: !!store.st.playing }
                Text { visible: nowPlaying.named; text: "Now Playing"; color: store.playerView ? ui.bright : ui.dim; font { family: ui.sans; pixelSize: ui.px(15); weight: store.playerView ? Font.DemiBold : Font.Normal } }
            }
            Beam { visible: store.playerView; anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: ui.px(10) } }
            HoverHandler { id: nowHover; cursorShape: Qt.PointingHandCursor }
            ToolTip.visible: !nowPlaying.named && nowHover.hovered
            ToolTip.text: "Now Playing"
            ToolTip.delay: 350
            TapHandler { onTapped: { store.showPlayer(); nav.searchFinished() } }
        }

    }

    }

    Rectangle {
        id: searchBox
        parent: tabs
        anchors.verticalCenter: parent.verticalCenter
        width: nav.searchExpanded ? Math.min(ui.px(240), nav.width * 0.4) : ui.px(34)
        height: ui.px(32)
        radius: height / 2
        color: nav.searchExpanded ? Qt.alpha(ui.deep, 0.5) : "transparent"
        border.width: nav.searchExpanded ? 1 : 0
        border.color: store.section === store.secSearch ? ui.here : ui.edge
        RowLight { anchors.fill: parent; visible: !nav.searchExpanded && (store.section === store.secSearch || searchHover.hovered); strength: store.section === store.secSearch ? 0.65 : 0.2 }
        Text {
            id: lens
            anchors { left: parent.left; leftMargin: ui.px(10); verticalCenter: parent.verticalCenter }
            text: store.sectionIcons[store.secSearch] || ""
            color: store.section === store.secSearch || searchHover.hovered ? ui.bright : ui.dim
            font { family: ui.mono; pixelSize: ui.px(18) }
        }
        TextInput {
            id: searchInput
            visible: nav.searchExpanded
            anchors { left: lens.right; leftMargin: ui.px(8); right: parent.right; rightMargin: ui.px(10); verticalCenter: parent.verticalCenter }
            text: store.query
            color: ui.fg
            font { family: ui.sans; pixelSize: ui.px(14) }
            selectByMouse: true
            clip: true
            onActiveFocusChanged: if (activeFocus) { store.switchTo(store.secSearch); store.searching = true }
            onTextEdited: { store.query = text; searchTimer.restart() }
            onAccepted: { searchTimer.stop(); store.runSearch(true); store.searching = false; nav.searchExpanded = false; nav.searchFinished() }
            Keys.onEscapePressed: { searchTimer.stop(); store.searching = false; nav.searchExpanded = false; nav.searchFinished() }
            Text {
                visible: !searchInput.text && !searchInput.activeFocus
                text: "Search"
                color: ui.dim
                font: searchInput.font
            }
        }
        TapHandler {
            enabled: !nav.searchExpanded
            onTapped: { nav.searchExpanded = true; searchInput.forceActiveFocus() }
        }
        HoverHandler { id: searchHover; cursorShape: Qt.PointingHandCursor }
        ToolTip.visible: !nav.searchExpanded && searchHover.hovered
        ToolTip.text: "Search"
        ToolTip.delay: 350
        Timer { id: searchTimer; interval: 350; onTriggered: store.runSearch(false) }
    }

}
