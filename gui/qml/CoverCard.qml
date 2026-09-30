// CoverCard is the cover on the stage as a card: a click turns it over,
// and its back tells about the song — title, artist, album, the album's
// facts and Apple's words about it, how long it is, where in the queue.
import QtQuick

Flipable {
    id: card
    property string url
    property bool flipped: false
    property bool reflect: false // on the floor of the coverflow
    property real reflection: 0.28
    readonly property var st: store.st
    // What the lists know of the album playing: its facts and note.
    readonly property var album: { store.rev; return flipped ? store.albumOf(st.album, st.artist) : null }

    front: Cover {
        width: card.width
        height: card.width
        url: card.url
        reflect: card.reflect
        reflection: card.reflection
    }
    back: Rectangle {
        width: card.width
        height: card.width
        radius: ui.coverRadius
        gradient: Gradient {
            GradientStop { position: 0; color: ui.tint(ui.deep, 0.22) }
            GradientStop { position: 1; color: ui.tint(ui.deep, 0.08) }
        }
        border { width: 1; color: Qt.alpha(ui.here, 0.5) }
        Rectangle { // the gloss
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 1 }
            height: parent.height * 0.4
            radius: parent.radius
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.alpha("white", 0.08) }
                GradientStop { position: 1; color: "transparent" }
            }
        }
        Column {
            anchors { fill: parent; margins: Math.max(ui.px(18), card.width * 0.08) }
            spacing: ui.px(6)
            Text {
                width: parent.width
                text: card.st.preview ? card.st.preview.title : (card.st.title || "")
                color: ui.bright
                wrapMode: Text.WordWrap
                maximumLineCount: 3
                elide: Text.ElideRight
                font { family: ui.sans; pixelSize: Math.max(ui.px(16), card.width * 0.06); weight: Font.DemiBold }
            }
            Text {
                width: parent.width
                text: card.st.preview ? card.st.preview.artist : (card.st.artist || "")
                color: ui.here
                elide: Text.ElideRight
                font { family: ui.sans; pixelSize: Math.max(ui.px(13), card.width * 0.04) }
            }
            Item { width: 1; height: ui.px(10) }
            Repeater {
                model: [
                    ["Album", card.st.album || ""],
                    ["", card.album ? card.album.info || "" : ""],
                    ["Length", card.st.dur > 0 ? Math.floor(card.st.dur / 60) + ":" + String(Math.floor(card.st.dur % 60)).padStart(2, "0") : ""],
                    ["Queue", card.st.length > 1 ? (card.st.index + 1) + " of " + card.st.length : ""],
                ].filter(x => x[1])
                Row {
                    required property var modelData
                    width: parent.width
                    spacing: ui.px(10)
                    Text {
                        width: ui.px(64)
                        text: modelData[0].toUpperCase()
                        color: ui.dim
                        font { family: ui.sans; pixelSize: ui.px(10); weight: Font.Bold; letterSpacing: 1.2 }
                        anchors.baseline: value.baseline
                    }
                    Text {
                        id: value
                        width: parent.width - ui.px(74)
                        text: modelData[1]
                        color: ui.fg
                        wrapMode: Text.WordWrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        font { family: ui.sans; pixelSize: ui.px(13) }
                    }
                }
            }
            Item { width: 1; height: ui.px(8) }
            Text {
                width: parent.width
                visible: !!(card.album && card.album.note)
                text: card.album ? (card.album.note || "") : ""
                color: ui.dim
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                maximumLineCount: Math.max(2, Math.floor((card.width * 0.3) / ui.px(18)))
                lineHeight: 1.25
                font { family: ui.sans; pixelSize: ui.px(12); italic: true }
            }
        }
    }

    transform: Rotation {
        id: turn
        origin.x: card.width / 2
        origin.y: card.width / 2
        axis { x: 0; y: 1; z: 0 }
        angle: card.flipped ? 180 : 0
        Behavior on angle { enabled: !ui.calm; NumberAnimation { duration: 520; easing.type: Easing.OutBack; easing.overshoot: 0.9 } }
    }

    TapHandler { onTapped: card.flipped = !card.flipped }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    // A new song shows its cover again.
    Connections { target: store; function onArtworkChanged() { card.flipped = false } }
}
