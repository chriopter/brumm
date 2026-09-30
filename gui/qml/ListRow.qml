// ListRow is one line of the list: a song or something that opens, with
// its cover and a second line under its name; or a shelf's heading, or a
// note about the view.
import QtQuick
import QtQuick.Effects

Item {
    id: r
    required property int index
    required property var modelData

    readonly property var track: modelData.track || null
    readonly property var item: modelData.item || null
    readonly property string head: modelData.head || ""
    readonly property string note: modelData.note || ""
    readonly property bool selected: store.sel === index
    readonly property bool playing: !!track && track.id === store.st.id && !store.st.preview
    // An album or playlist the song playing was started from.
    readonly property bool holdsPlaying: !!item && !!store.st.source && store.itemKey(item) === store.st.source
    // An album's songs share its cover: they show their numbers instead.
    readonly property bool albumView: { store.rev; return !!store.view && !!store.view.item && store.view.item.kind === "album" }
    readonly property string artwork: track ? (track.artwork || "") : item ? (item.artwork || "") : ""
    readonly property string sub: {
        if (track) return r.albumView && store.view.item.artist === track.artist ? "" : track.artist
        if (!item) return ""
        const kind = { album: "Album", playlist: "Playlist", artist: "Artist", station: "Station", term: "Search", shelf: "" }[item.kind] || ""
        return [kind, item.artist].filter(x => x).join(" · ")
    }

    height: head ? ui.px(index === 0 ? 30 : 48)
          : note ? noteText.implicitHeight + ui.px(14)
          : (track || item) ? (albumView ? ui.px(40) : ui.px(50)) : ui.px(10)

    // The selection, as the XMB lights it: a glow behind the row; hovering,
    // a faint one. While the keys are in the sections on the left, the list
    // gives up its light.
    RowLight {
        visible: (r.selected && !store.sideFocus) || hover.hovered
        anchors { fill: parent; leftMargin: ui.px(8) }
        color: r.selected && !store.sideFocus ? ui.here : ui.bright
        strength: r.selected && !store.sideFocus ? 1 : 0.25
    }

    HoverHandler { id: hover; enabled: !!(r.track || r.item) }
    TapHandler {
        enabled: !!(r.track || r.item)
        onTapped: store.activateAt(r.index) // one click plays or opens
    }

    // A shelf's heading.
    Text {
        visible: !!r.head
        anchors { left: parent.left; leftMargin: ui.px(12); bottom: parent.bottom; bottomMargin: ui.px(7) }
        text: r.head.toUpperCase()
        color: ui.here
        font { family: ui.sans; pixelSize: ui.px(11); weight: Font.Bold; letterSpacing: 1.6 }
    }

    // A note: an album's facts, Apple's words about it.
    Text {
        id: noteText
        visible: !!r.note
        anchors { left: parent.left; right: parent.right; leftMargin: ui.px(12); rightMargin: ui.px(12); verticalCenter: parent.verticalCenter }
        text: r.note
        color: ui.dim
        wrapMode: Text.WordWrap
        maximumLineCount: 4
        elide: Text.ElideRight
        lineHeight: 1.2
        font { family: ui.sans; pixelSize: ui.px(12) }
    }

    // A song or an item.
    Item {
        id: line
        visible: !!(r.track || r.item)
        anchors { fill: parent; leftMargin: ui.px(8); rightMargin: ui.px(12) }

        // The cover, or the song's number on its album.
        Item {
            id: lead
            width: r.albumView ? ui.px(28) : ui.px(36)
            height: width
            anchors.verticalCenter: parent.verticalCenter
            scale: r.selected && !r.albumView ? 1.12 : 1
            Behavior on scale { enabled: !ui.calm; NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
            Rectangle {
                anchors.fill: parent
                visible: !r.albumView
                radius: r.item && r.item.kind === "artist" ? width / 2 : ui.px(5)
                color: ui.glassHi
                border { width: 1; color: ui.edge }
                Text { // until the cover shows; not for one on its way from the cache
                    anchors.centerIn: parent
                    visible: thumb.status !== Image.Ready && (thumb.slow || thumb.status !== Image.Loading)
                    color: ui.dim
                    font { family: ui.mono; pixelSize: ui.px(15) }
                    text: r.track ? "󰎈" : !r.item ? ""
                        : ({ station: "󰐹", artist: "󰠃", playlist: "󰲸", term: "󰍉", shelf: "󰄨" })[r.item.kind] || "󰀥"
                }
            }
            Image {
                id: thumb
                anchors.fill: parent
                visible: !r.albumView && !!r.artwork
                source: visible ? "image://cover/" + encodeURIComponent(r.artwork) : ""
                sourceSize: Qt.size(ui.px(72), ui.px(72))
                asynchronous: true
                fillMode: Image.PreserveAspectCrop
                smooth: true
                opacity: status === Image.Ready ? 1 : 0
                // A cover from the cache is there in a frame or two and just
                // shows; one that takes longer, fetched, fades in.
                property bool slow: false
                onSourceChanged: slow = false
                Timer { interval: 120; running: thumb.status === Image.Loading; onTriggered: thumb.slow = true }
                Behavior on opacity { enabled: !ui.calm && thumb.slow; NumberAnimation { duration: 180 } }
            }
            Text {
                anchors.centerIn: parent
                visible: r.albumView && !r.playing
                text: r.track && r.track.number ? r.track.number : ""
                color: ui.dim
                font { family: ui.sans; pixelSize: ui.px(12) }
            }
            Item { // where its equalizer stands
                id: leadSpot
                anchors.centerIn: parent
                visible: r.playing && r.albumView
                width: eq.width
                height: eq.height
            }
        }

        Column {
            anchors { left: lead.right; leftMargin: ui.px(12); right: tail.left; rightMargin: ui.px(12); verticalCenter: parent.verticalCenter }
            spacing: ui.px(2)
            Text {
                width: parent.width
                text: r.track ? r.track.title : r.item ? r.item.name : ""
                color: r.playing ? ui.here : r.selected ? ui.bright : ui.fg
                style: ui.lift
                styleColor: ui.liftColor
                font { family: ui.sans; pixelSize: ui.px(r.selected ? 15 : 14); weight: r.selected || r.playing ? Font.DemiBold : Font.Medium }
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: text !== ""
                text: r.sub
                color: ui.dim
                font { family: ui.sans; pixelSize: ui.px(12) }
                elide: Text.ElideRight
            }
        }

        Row {
            id: tail
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: ui.px(10)
            Item { // where its equalizer stands
                id: tailSpot
                anchors.verticalCenter: parent.verticalCenter
                visible: (r.playing && !r.albumView) || r.holdsPlaying
                width: eq.width
                height: eq.height
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                readonly property int mark: { store.ratingRev; const id = r.track ? r.track.id : r.item ? r.item.id : ""; return store.rating[id] || 0 }
                visible: mark !== 0
                text: mark > 0 ? "♥" : "󰔑"
                color: mark > 0 ? ui.heart : ui.dim
                font { family: ui.mono; pixelSize: ui.px(13) }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: !!r.track && r.width >= ui.px(380) // a narrow list keeps the titles, not the times
                text: r.track ? clock(r.track.duration) : ""
                color: ui.dim
                font { family: ui.sans; pixelSize: ui.px(12); features: { "tnum": 1 } }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: !!r.item && r.item.kind !== "station" && r.item.kind !== "term"
                text: "›"
                color: r.selected ? ui.fg : ui.dim
                font { family: ui.sans; pixelSize: ui.px(18) }
            }
        }
    }

    // The playing row's equalizer stands over the list, not in it: the
    // list is drawn once into its fade (Browser.qml), and bars bobbing in
    // it would draw all of it again eight times a second. It keeps to its
    // spot in the row and fades at the list's edges as the row does.
    readonly property var list: ListView.view
    property bool pooled: false
    ListView.onPooled: pooled = true
    ListView.onReused: pooled = false
    Equalizer {
        id: eq
        readonly property bool inLead: leadSpot.visible
        parent: r.list ? r.list.overlay : r
        visible: !r.pooled && !!r.list && (leadSpot.visible || tailSpot.visible) && y + height > 0 && y < parent.height
        running: !!store.st.playing && (r.playing || r.holdsPlaying)
        x: r.list ? Math.round(r.x + r.list.contentItem.x + line.x + (inLead ? lead.x + (lead.width - width) / 2 : tail.x + tailSpot.x)) : 0
        y: r.list ? Math.round(r.y + r.list.contentItem.y + (r.height - height) / 2) : 0
        opacity: r.list ? r.list.fadeAt(y + height / 2) : 1
    }

    function clock(sec) {
        sec = Math.max(0, Math.floor(sec || 0))
        const m = Math.floor(sec / 60), s = sec % 60
        return m + ":" + (s < 10 ? "0" : "") + s
    }
}
