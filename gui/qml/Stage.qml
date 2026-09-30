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
        width: Math.max(stage.coverSize, Math.min(stage.width, ui.px(540)))
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
                    width: Math.min(implicitWidth, parent.width - rate.width - ui.px(16))
                    text: stage.title
                    color: ui.bright
                    style: ui.lift
                    styleColor: ui.liftColor
                    font { family: ui.sans; pixelSize: ui.px(26); weight: Font.DemiBold }
                    elide: Text.ElideRight
                }
                // Love and dislike, glass as the transport, lit when set.
                Row {
                    id: rate
                    anchors { right: parent.right; verticalCenter: titleText.verticalCenter }
                    spacing: ui.px(10)
                    visible: !stage.preview
                    readonly property int mark: { store.ratingRev; return store.rating[stage.st.id] || 0 }
                    Orb { size: ui.px(34); icon: rate.mark > 0 ? "󰋑" : "󰋕"; on: rate.mark > 0; onClicked: store.ratePlaying(1) }
                    Orb { size: ui.px(34); icon: "󰔑"; on: rate.mark < 0; onClicked: store.ratePlaying(-1) }
                }
                // A right click on the title copies the song's link.
                TapHandler { acceptedButtons: Qt.RightButton; onTapped: store.linkPlaying() }
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

            Item { width: 1; height: ui.px(16) }
            Progress { width: parent.width }
            Item { width: 1; height: ui.px(12) }
            Controls { width: parent.width }
        }
    }
}
