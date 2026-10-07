// Stage is the right side: the cover on its stand, what plays, where it
// is, the transport; what played and what comes next beside the cover.
import QtQuick
import QtQuick.Effects

Item {
    id: stage
    clip: false
    property bool compact: false
    readonly property real playbackTop: col.y + info.y - ui.px(18)

    readonly property var st: store.st
    readonly property bool preview: !!st.preview
    readonly property bool has: !!(st.title || st.preview)
    readonly property string title: preview ? st.preview.title : (st.title || "")
    readonly property string artist: preview ? st.preview.artist : (st.artist || "")
    readonly property string album: preview ? "" : (st.album || "")

    // The info under the cover takes its height, the cover and its floor
    // the rest, the cover up to most of the width: its neighbours stand
    // in what is left beside it.
    readonly property real coverHeight: store.wall ? Math.max(coverSize, stage.height - info.implicitHeight - ui.px(30)) : Math.round(coverSize * 1.2 + ui.px(4))
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
            visible: !stage.compact && !store.wall
            anchors.horizontalCenter: parent.horizontalCenter
            width: stage.width
            size: stage.coverSize
            url: win.artwork
        }
        CoverWall {
            visible: !stage.compact && store.wall
            anchors.horizontalCenter: parent.horizontalCenter
            width: stage.width
            height: Math.max(stage.coverSize, stage.height - info.implicitHeight - ui.px(30))
            size: stage.coverSize
            url: win.artwork
        }

        Item { visible: stage.compact; width: 1; height: stage.coverHeight }
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
                // Add (to a playlist, the library) and love, glass as the
                // transport; love lit when set.
                Row {
                    id: rate
                    anchors { right: parent.right; verticalCenter: titleText.verticalCenter }
                    spacing: ui.px(10)
                    visible: !stage.preview
                    readonly property int mark: { store.ratingRev; return store.rating[stage.st.id] || 0 }
                    Orb { id: add; size: ui.px(34); icon: "󰇙"; on: store.addOpen; onClicked: store.addOpen = !store.addOpen }
                    Orb { size: ui.px(34); icon: rate.mark > 0 ? "󰋑" : "󰋕"; on: rate.mark > 0; onClicked: store.ratePlaying(1) }
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
    RectangularShadow {
        visible: stage.compact && stage.has && ui.effects
        x: col.x - ui.px(18)
        y: col.y + info.y - ui.px(18)
        width: info.width + ui.px(36)
        height: info.height + ui.px(36)
        radius: ui.radius
        blur: ui.px(30)
        color: Qt.alpha("black", 0.3)
        z: -1
    }
    Item {
        id: glassPanel
        visible: stage.compact && stage.has
        x: col.x - ui.px(18)
        y: col.y + info.y - ui.px(18)
        width: info.width + ui.px(36)
        height: info.height + ui.px(36)
        z: -1
        ShaderEffectSource {
            id: glassSource
            sourceItem: glassPanel.visible && ui.effects ? backgroundLayer : null
            readonly property point at: { glassPanel.x; glassPanel.y; stage.x; stage.y; body.x; body.y; return glassPanel.mapToItem(backgroundLayer, 0, 0) }
            sourceRect: Qt.rect(at.x, at.y, glassPanel.width, glassPanel.height)
            textureSize: Qt.size(Math.max(1, Math.ceil(width / 2)), Math.max(1, Math.ceil(height / 2)))
            anchors.fill: parent
            visible: false
            live: glassPanel.visible && ui.effects && ui.awake
        }
        Rectangle { id: glassMask; anchors.fill: parent; radius: ui.radius; color: "white"; layer.enabled: true; visible: false }
        MultiEffect {
            anchors.fill: parent
            visible: ui.effects
            source: glassSource
            autoPaddingEnabled: false
            blurEnabled: true
            blur: 0.8
            blurMax: 32
            saturation: 0.2
            maskEnabled: true
            maskSource: glassMask
        }
        ShaderEffectSource {
            id: contentGlassSource
            sourceItem: glassPanel.visible && ui.effects ? browser : null
            textureSize: Qt.size(Math.max(1, Math.ceil(width / 2)), Math.max(1, Math.ceil(height / 2)))
            readonly property point at: { glassPanel.x; glassPanel.y; stage.x; stage.y; browser.x; browser.y; return glassPanel.mapToItem(browser, 0, 0) }
            sourceRect: Qt.rect(at.x, at.y, glassPanel.width, glassPanel.height)
            anchors.fill: parent
            visible: false
            live: glassPanel.visible && ui.effects && ui.awake
        }
        MultiEffect {
            anchors.fill: parent
            visible: ui.effects
            source: contentGlassSource
            autoPaddingEnabled: false
            blurEnabled: true
            blur: 0.8
            blurMax: 32
            maskEnabled: true
            maskSource: glassMask
        }
        Rectangle { anchors.fill: parent; radius: ui.radius; color: Qt.alpha(ui.deep, 0.2) }
        TapHandler {}
        GlassShape { anchors.fill: parent; radius: ui.radius; tint: ui.here; hot: 0.25 }
    }
    // Song actions, a card of glass under the three dots.
    Rectangle {
        id: addCard
        parent: win.contentItem
        visible: store.addOpen && stage.visible && stage.has
        z: 60
        readonly property point at: { store.addOpen; win.width; win.height; return add.mapToItem(win.contentItem, 0, 0) }
        width: ui.px(210)
        height: addRows.implicitHeight + ui.px(12)
        x: Math.min(at.x + add.width / 2 - width / 2, win.width - width - ui.px(8))
        y: Math.max(ui.px(8), Math.min(at.y + add.height + ui.px(10), win.height - height - ui.px(8)))
        radius: ui.px(12)
        color: ui.tint(ui.deep, 0.08)
        border { width: 1; color: Qt.alpha(ui.bright, 0.14) }
        TapHandler {}
        Column {
            id: addRows
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: ui.px(6) }
            Repeater {
                model: [["󰲸", "Add to Playlist…", "P", () => store.pickPlaying()],
                        ["󰋚", "Add to Library", "i", () => store.libraryPlaying()],
                        ["󰐹", "Start Radio", "R", () => store.radioPlaying()],
                        ["󰔑", "Suggest Less", "d", () => store.ratePlaying(-1)]]
                Item {
                    required property var modelData
                    width: addRows.width
                    height: ui.px(34)
                    RowLight { anchors.fill: parent; visible: rowHover.hovered }
                    Text {
                        id: glyph
                        anchors { left: parent.left; leftMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        text: modelData[0]; color: rowHover.hovered ? ui.here : ui.dim
                        font { family: ui.mono; pixelSize: ui.px(15) }
                    }
                    Text {
                        anchors { left: glyph.right; leftMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        text: modelData[1]; color: rowHover.hovered ? ui.bright : ui.fg
                        font { family: ui.sans; pixelSize: ui.px(13) }
                    }
                    Text {
                        anchors { right: parent.right; rightMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        text: modelData[2]; color: ui.dim
                        font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold }
                    }
                    HoverHandler { id: rowHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: { store.addOpen = false; modelData[3]() } }
                }
            }
        }
    }
}
