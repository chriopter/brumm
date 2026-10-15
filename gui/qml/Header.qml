// Header runs across the top: where you are, the stack's last title large
// and the way there small, and under it a glowing line.
import QtQuick
import QtQuick.Controls

Item {
    id: h
    height: ui.px(36)

    Item {
        id: crumbs
        anchors { left: parent.left; leftMargin: ui.px(2); verticalCenter: parent.verticalCenter; verticalCenterOffset: -ui.px(4) }
        width: Math.min(trail.width, h.width - (albumActions.visible ? albumActions.width + ui.px(12) : ui.px(4)) - (sortButton.visible ? sortButton.width + ui.px(8) : 0))
        height: trail.height
        clip: true

        Row {
            id: trail
            x: Math.min(0, crumbs.width - width)
            Repeater {
                id: parts
                model: { store.rev; return store.stack().map(v => v.title) }
                Row {
                    id: part
                    required property int index
                    required property string modelData
                    readonly property bool ancestor: index < parts.count - 1
                    Text {
                        text: part.modelData
                        color: part.ancestor ? Qt.alpha(ui.bright, 0.5) : ui.bright
                        style: ui.lift
                        styleColor: ui.liftColor
                        font { family: ui.sans; pixelSize: ui.px(15); weight: Font.DemiBold; underline: click.containsMouse }
                        MouseArea {
                            id: click
                            anchors.fill: parent
                            enabled: part.ancestor
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: { win.focusPlayer(); store.jumpToCrumb(part.index) }
                        }
                    }
                    Text {
                        visible: part.ancestor
                        text: "  ›  "
                        color: Qt.alpha(ui.bright, 0.5)
                        style: ui.lift
                        styleColor: ui.liftColor
                        font { family: ui.sans; pixelSize: ui.px(15); weight: Font.DemiBold }
                    }
                }
            }
        }
    }

    readonly property var album: { store.rev; return store.view && store.view.item && store.view.item.kind === "album" ? store.view.item : null }
    Orb {
        id: albumActions
        objectName: "albumActions"
        visible: !!h.album
        anchors { left: crumbs.right; leftMargin: ui.px(8); verticalCenter: crumbs.verticalCenter }
        size: ui.px(28)
        icon: "󰇙"
        tooltip: "Album actions"
        onClicked: { win.focusPlayer(); albumMenu.open() }
    }
    Menu {
        id: albumMenu
        parent: albumActions
        y: albumActions.height
        width: ui.px(210)
        padding: ui.px(6)
        property var entry: null
        onAboutToShow: entry = h.album
        ContextAction { text: "Play album"; onTriggered: if (albumMenu.entry) store.playItem(albumMenu.entry) }
        ContextAction { text: "Add to Playlist…"; visible: !!store.capabilities.playlistWrite; onTriggered: if (albumMenu.entry) store.openPicker({ item: albumMenu.entry }) }
        ContextAction { text: "Add to Library"; visible: !!store.capabilities.collectionWrite && (store.capabilities.collectionKinds || []).includes("album"); onTriggered: if (albumMenu.entry) store.addToLibrary({ item: albumMenu.entry }) }
        ContextAction { text: "Favorite"; visible: (!!store.capabilities.preference && (store.capabilities.favoriteKinds || []).includes("album")) || (!!store.capabilities.ratings && (store.capabilities.ratingKinds || []).includes("album")); onTriggered: if (albumMenu.entry) store.rate(1, { item: albumMenu.entry }) }
        background: Rectangle { radius: ui.px(12); color: ui.tint(ui.deep, 0.08); border { width: 1; color: ui.edge } }
    }

    Orb {
        id: sortButton
        visible: { store.rev; return !!store.view && !!store.view.item && store.view.item.kind === "playlist" }
        anchors { right: parent.right; verticalCenter: crumbs.verticalCenter }
        size: ui.px(28)
        icon: "󰒺"
        tooltip: "Sort playlist view"
        onClicked: sortMenu.open()
    }
    Menu {
        id: sortMenu
        parent: sortButton
        x: sortButton.width - width
        y: sortButton.height
        width: ui.px(190)
        padding: ui.px(6)
        Repeater {
            model: [{key:"original",label:"Playlist order"}, {key:"title",label:"Title"}, {key:"artist",label:"Artist"}, {key:"album",label:"Album"}, {key:"duration",label:"Duration"}]
            ContextAction {
                required property var modelData
                text: { store.rev; return (store.view && store.view.sortMode === modelData.key ? "✓  " : "    ") + modelData.label }
                onTriggered: store.sortPlaylist(modelData.key)
            }
        }
        background: Rectangle { radius: ui.px(12); color: ui.tint(ui.deep, 0.08); border.color: ui.edge }
    }

    Beam { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } }
}
