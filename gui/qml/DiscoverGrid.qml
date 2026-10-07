import QtQuick

Flickable {
    id: grid
    clip: true
    contentHeight: tiles.height
    boundsBehavior: Flickable.StopAtBounds
    property string pageKey: ""
    readonly property real reservedX: playbackStage.x - browser.x - ui.px(18)
    readonly property real reservedY: playbackStage.y + playbackStage.playbackTop - browser.y
    readonly property int columns: Math.max(2, Math.floor(width / ui.px(210)))
    readonly property real tileWidth: (width - (columns - 1) * ui.px(20)) / columns
    Flow {
        id: tiles
        width: grid.width
        spacing: ui.px(20)
        Repeater {
            id: cards
            model: store.rows
            delegate: Item {
                id: card
                required property var modelData
                required property int index
                readonly property var entry: modelData.item || modelData.track
                readonly property bool heading: !entry
                readonly property bool selected: index === store.sel
                readonly property bool behindPlayer: x + width > grid.reservedX && y - grid.contentY + height > grid.reservedY
                opacity: 1
                readonly property bool nearViewport: y + height >= grid.contentY - grid.height && y <= grid.contentY + 2 * grid.height
                readonly property string artwork: entry ? entry.artwork || "" : ""
                width: heading ? tiles.width : grid.tileWidth
                height: heading ? caption.implicitHeight + ui.px(20) : (artwork ? width + ui.px(58) : ui.px(94))
                RowLight { anchors.fill: parent; visible: !card.heading && (card.selected || hover.hovered) }
                Rectangle {
                    id: cover
                    visible: !!card.artwork
                    width: parent.width
                    height: width
                    radius: ui.coverRadius
                    color: Qt.alpha(ui.bright, 0.04)
                    clip: true
                    Image { anchors.fill: parent; sourceSize: Qt.size(Math.ceil(width), Math.ceil(height)); source: card.nearViewport && card.artwork ? "image://cover/" + encodeURIComponent(card.artwork) : ""; fillMode: Image.PreserveAspectCrop; asynchronous: true }
                }
                Text {
                    id: caption
                    x: card.heading ? 0 : ui.px(8)
                    y: card.heading ? ui.px(10) : (card.artwork ? cover.height + ui.px(10) : ui.px(18))
                    width: parent.width - x * 2
                    text: card.entry ? card.entry.name || card.entry.title || "" : (modelData.head || modelData.note || "") + (modelData.headRoute ? "  ›" : "")
                    color: card.selected ? ui.here : ui.bright
                    font { family: ui.sans; pixelSize: ui.px(card.heading ? 20 : 16); weight: Font.DemiBold }
                    wrapMode: card.heading ? Text.WordWrap : Text.NoWrap
                    elide: Text.ElideRight
                }
                Text {
                    anchors { left: caption.left; right: caption.right; top: caption.bottom; topMargin: ui.px(5) }
                    text: card.entry ? card.entry.artist || (card.entry.kind === "shelf" ? "Explore ›" : card.entry.kind || "") : ""
                    color: ui.dim
                    font { family: ui.sans; pixelSize: ui.px(13) }
                    elide: Text.ElideRight
                }
                HoverHandler { enabled: card.heading && !!modelData.headRoute; cursorShape: Qt.PointingHandCursor }
                TapHandler { enabled: card.heading && !!modelData.headRoute; onTapped: { win.focusPlayer(); store.openItem({ kind: "shelf", name: modelData.head, id: modelData.headRoute, route: modelData.headRoute, catalog: true }) } }
                HoverHandler { id: hover; enabled: !card.heading && !card.behindPlayer; cursorShape: Qt.PointingHandCursor }
                TapHandler { enabled: !card.heading && !card.behindPlayer; onTapped: { win.focusPlayer(); store.activateAt(card.index) } }
                TapHandler { enabled: !card.heading && !card.behindPlayer; acceptedButtons: Qt.RightButton; onTapped: eventPoint => { win.focusPlayer(); win.rowContext(card.index, card, eventPoint.position) } }
            }
        }
    }
    Connections {
        target: store
        function onRowsChanged() {
            const key = store.view ? store.view.key : ""
            if (key !== grid.pageKey) { grid.pageKey = key; grid.contentY = 0 }
        }
        function onSelMoved() {
            Qt.callLater(() => {
                const card = cards.itemAt(store.sel)
                if (!card) return
                if (card.y < grid.contentY) grid.contentY = card.y
                else {
                    const bottom = card.x + card.width > grid.reservedX ? Math.min(grid.height, grid.reservedY) : grid.height
                    if (card.y + card.height > grid.contentY + bottom)
                        grid.contentY = Math.max(0, card.y + card.height - bottom)
                }
            })
        }
    }
}
