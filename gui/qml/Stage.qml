// Stage is the right side: the cover on its stand, what plays, where it
// is, the transport; what played and what comes next beside the cover.
import QtQuick

Item {
    id: stage
    clip: false

    readonly property var st: store.st
    readonly property bool preview: !!st.preview
    readonly property bool has: !!(st.title || st.preview)
    readonly property string title: preview ? st.preview.title : (st.title || "")
    readonly property string artist: preview ? st.preview.artist : (st.artist || "")
    readonly property string album: preview ? "" : (st.album || "")

    // The info under the cover takes its height, the cover and its floor
    // the rest, the cover up to most of the width: its neighbours stand
    // in what is left beside it.
    readonly property int coverSize: Math.max(ui.px(120), Math.min(width * 0.5, ui.px(500), (height - info.implicitHeight - ui.px(56)) / 1.2))

    // A message instead of the stage: starting up, signed out, broken.
    Column {
        anchors.centerIn: parent
        width: parent.width - ui.px(40)
        visible: !stage.has
        spacing: ui.px(14)
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "󰎈"
            color: ui.edge
            font { family: ui.mono; pixelSize: ui.px(64) }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            color: ui.dim
            lineHeight: 1.3
            font { family: ui.sans; pixelSize: ui.px(15) }
            text: {
                if (store.st.status === "logged-out") return "Not signed in to Apple Music.\nPress L to sign in in your browser."
                if (store.st.status === "error") return store.st.message || "brumm could not start its player."
                if (store.st.status === "starting") return store.st.message || "Starting the player…"
                return "Nothing plays.\nPick a song and press enter."
            }
        }
    }

    Column {
        id: col
        visible: stage.has
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(0, Math.round((stage.height - implicitHeight) / 2))
        width: Math.max(stage.coverSize, Math.min(stage.width, ui.px(440)))
        spacing: 0

        // The cover in a flow of what played and comes next, or in a wall
        // of covers, as the menu says.
        CoverFlow {
            visible: !store.wall
            anchors.horizontalCenter: parent.horizontalCenter
            width: stage.width
            size: stage.coverSize
            url: win.artwork
        }
        CoverWall {
            visible: store.wall
            anchors.horizontalCenter: parent.horizontalCenter
            width: stage.width
            height: Math.max(stage.coverSize, stage.height - info.implicitHeight - ui.px(30))
            size: stage.coverSize
            url: win.artwork
        }

        Item { width: 1; height: ui.px(10) }

        Column {
            id: info
            width: col.width
            spacing: ui.px(4)

            // The title, and where in the queue it is.
            Item {
                width: parent.width
                height: titleText.height
                Text {
                    id: titleText
                    anchors.left: parent.left
                    width: Math.min(implicitWidth, parent.width - tag.width - ui.px(16) - (heart.visible ? heart.width + ui.px(10) : 0))
                    text: stage.title
                    color: ui.bright
                    style: ui.lift
                    styleColor: ui.liftColor
                    font { family: ui.sans; pixelSize: ui.px(26); weight: Font.DemiBold }
                    elide: Text.ElideRight
                }
                Text {
                    id: heart
                    anchors { left: titleText.right; leftMargin: ui.px(10); verticalCenter: titleText.verticalCenter }
                    readonly property int mark: { store.ratingRev; return stage.preview ? 0 : store.rating[stage.st.id] || 0 }
                    visible: mark !== 0
                    text: mark > 0 ? "♥" : "󰔑"
                    color: mark > 0 ? ui.heart : ui.dim
                    font { family: ui.mono; pixelSize: ui.px(18) }
                }
                Rectangle {
                    id: tag
                    anchors { right: parent.right; verticalCenter: titleText.verticalCenter }
                    width: tagText.implicitWidth + ui.px(16)
                    height: tagText.implicitHeight + ui.px(6)
                    radius: height / 2
                    visible: tagText.text !== ""
                    color: stage.preview ? ui.here : Qt.alpha(ui.bright, 0.07)
                    Text {
                        id: tagText
                        anchors.centerIn: parent
                        text: stage.preview ? "PREVIEW" : stage.st.length > 1 && stage.st.index >= 0 ? (stage.st.index + 1) + " / " + stage.st.length : ""
                        color: stage.preview ? ui.deep : ui.dim
                        font { family: ui.sans; pixelSize: ui.px(11); weight: Font.DemiBold; letterSpacing: stage.preview ? 1.2 : 0; features: { "tnum": 1 } }
                    }
                }
            }

            // The artist and the album: each opens with a click (A, a).
            Row {
                width: parent.width
                clip: true
                Text {
                    id: artistText
                    text: stage.artist
                    color: artistHover.hovered && !stage.preview ? ui.bright : ui.fg
                    font { family: ui.sans; pixelSize: ui.px(16); underline: artistHover.hovered && !stage.preview }
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, parent.width)
                    HoverHandler { id: artistHover; cursorShape: stage.preview ? Qt.ArrowCursor : Qt.PointingHandCursor }
                    TapHandler { onTapped: if (!stage.preview) store.lookup("artist", store.st.id) }
                }
                Text {
                    visible: stage.album !== "" && stage.album !== stage.title
                    text: "  ·  "
                    color: ui.dim
                    font { family: ui.sans; pixelSize: ui.px(16) }
                }
                Text {
                    visible: stage.album !== "" && stage.album !== stage.title
                    text: stage.album
                    color: albumHover.hovered ? ui.fg : ui.dim
                    font { family: ui.sans; pixelSize: ui.px(16); underline: albumHover.hovered }
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, parent.width - artistText.width - ui.px(40))
                    HoverHandler { id: albumHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: store.lookup("album", store.st.id) }
                }
            }

            // What to do with this song: small glass buttons that open to
            // show their keys when pointed at.
            Row {
                visible: !stage.preview
                anchors.left: parent.left
                anchors.leftMargin: -ui.px(10)
                spacing: 0
                readonly property int mark: { store.ratingRev; return store.rating[stage.st.id] || 0 }
                MorphButton { icon: parent.mark > 0 ? "󰋑" : "󰋕"; label: parent.mark > 0 ? "Loved" : "Love"; key: "*"; lit: parent.mark > 0; onClicked: store.ratePlaying(1) }
                MorphButton { icon: "󰔑"; label: parent.mark < 0 ? "Disliked" : "Dislike"; key: "d"; lit: parent.mark < 0; onClicked: store.ratePlaying(-1) }
                MorphButton { icon: "󰐒"; label: "Playlist"; key: "P"; onClicked: store.pickPlaying() }
                MorphButton { icon: "󰋚"; label: "Library"; key: "i"; visible: !(stage.st.id || "").startsWith("i."); onClicked: store.libraryPlaying() }
                MorphButton { icon: "󰐹"; label: "Radio"; key: "R"; onClicked: store.radioPlaying() }
                MorphButton { icon: "󰌷"; label: "Link"; key: "y"; onClicked: store.linkPlaying() }
            }

            Item { width: 1; height: ui.px(10) }
            Progress { width: parent.width }
            Item { width: 1; height: ui.px(12) }
            Controls { width: parent.width }
        }
    }
}
