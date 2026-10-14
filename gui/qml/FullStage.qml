// FullStage is f: the visualizer over the whole window, its thirteen styles
// named and ordered as the terminal player's, dissolving from one into the
// next every minute (a: stays), with what plays in a strip along the
// bottom. It listens to the daemon's sound only while it shows.
import QtQuick
import QtQuick.Window
import Brumm

Item {
    id: fs

    readonly property var st: store.st
    readonly property bool preview: !!st.preview
    readonly property bool shown: store.full
    // Its styles are shaders: without them (the software renderer) there
    // is nothing to draw, nothing to hear. Minimized, the daemon need not
    // send the sound either.
    readonly property bool moving: shown && ui.effects && win.visibility !== Window.Minimized
    onMovingChanged: daemon.listen(moving)

    // The textures every style reads (see viz/VizEffect.qml), fresh each
    // tick, written over in place.
    VizTexture { id: specTex; audio: vizAudio }
    VizTexture { id: waveTex; audio: vizAudio; wave: true }

    // The listener moves on once a frame while the visualizer shows.
    FrameAnimation {
        running: fs.moving
        onTriggered: vizAudio.tick(frameTime, !!store.st.playing)
    }

    // Two slots take turns: the new style fades in over the old one.
    property bool onA: true
    component Slot: Loader {
        id: slot
        property bool front
        property int style: -1
        anchors.fill: parent
        active: fs.shown && ui.effects && style >= 0 && (front || opacity > 0)
        // On a large or dense screen the styles are drawn with about three
        // million pixels and scaled up: they are soft light, and four times
        // the pixels would cost four times the GPU.
        readonly property real shrink: Math.min(1, Math.sqrt(3.0e6 / Math.max(1, width * height * Screen.devicePixelRatio * Screen.devicePixelRatio)))
        layer.enabled: ui.effects && shrink < 1
        layer.smooth: true
        layer.textureSize: Qt.size(Math.round(width * Screen.devicePixelRatio * shrink), Math.round(height * Screen.devicePixelRatio * shrink))
        opacity: front ? 1 : 0
        z: front ? 1 : 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 1500; easing.type: Easing.InOutQuad } }
        source: style >= 0 ? "viz/" + store.vizFiles[style] + ".qml" : ""
        onLoaded: {
            item.spectrum = specTex
            item.wave = waveTex
            item.running = Qt.binding(() => slot.opacity > 0 && fs.moving)
        }
    }
    Slot { id: slotA; front: fs.onA }
    Slot { id: slotB; front: !fs.onA }

    Connections {
        target: store
        function onVizStyleChanged() { fs.show(store.vizStyle, !store.vizQuick) }
        function onFullChanged() { if (store.full) fs.show(store.vizStyle, false) }
    }
    // show puts style i up: dissolving into it, or at once.
    function show(i, fade) {
        const front = onA ? slotA : slotB
        if (front.style === i) return
        nameShown.restart()
        if (!fade || ui.calm) { front.style = i; return }
        onA = !onA
        ;(onA ? slotA : slotB).style = i
    }

    // The style's name, large for a moment when it changes.
    Text {
        id: name
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: ui.px(40) }
        z: 5
        text: store.vizNames[store.vizStyle] || ""
        color: ui.bright
        opacity: 0
        font { family: ui.sans; pixelSize: ui.px(34); weight: Font.Light; letterSpacing: 8 }
        SequentialAnimation on opacity {
            id: nameShown
            running: false
            NumberAnimation { to: 0.9; duration: 300 }
            PauseAnimation { duration: 1400 }
            NumberAnimation { to: 0; duration: 900 }
        }
    }

    // The minutely change, unless the options keep the style.
    // A minute from the last change, as in the terminal player: a style
    // picked by hand, or the change switched back on, starts it over.
    Timer {
        id: minutely
        interval: 60000
        repeat: true
        running: fs.moving && !daemon.options.no_viz_cycle && !store.vizList
        onTriggered: store.vizStep(1)
    }
    Connections {
        target: store
        function onVizStyleChanged() { minutely.restart() }
    }
    Connections {
        target: daemon
        function onOptionsChanged() { minutely.restart() }
    }

    // x: the controls fade out while the mouse rests, and come back as
    // soon as it moves; with the list of styles open they stay.
    property bool resting: false
    readonly property bool bare: daemon.options.viz_bare && resting && !store.vizList
    Timer { id: rest; interval: 2500; running: fs.shown; onTriggered: fs.resting = true }
    function stir() { resting = false; rest.restart() }
    onShownChanged: stir()
    HoverHandler {
        cursorShape: fs.bare ? Qt.BlankCursor : Qt.ArrowCursor
        onPointChanged: fs.stir()
    }

    // What plays, in a strip along the bottom.
    Rectangle {
        id: strip
        z: 5
        opacity: fs.bare ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 600; easing.type: Easing.InOutQuad } }
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: ui.px(92)
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.6; color: Qt.alpha(ui.deep, 0.75) }
            GradientStop { position: 1; color: Qt.alpha(ui.deep, 0.9) }
        }
        Image {
            id: thumb
            visible: !win.compact
            anchors { left: parent.left; leftMargin: ui.gap; bottom: parent.bottom; bottomMargin: ui.px(18) }
            width: ui.px(48)
            height: width
            source: win.artwork ? "image://cover/" + encodeURIComponent(win.artwork) : ""
            sourceSize: Qt.size(ui.px(96), ui.px(96))
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
        Column {
            visible: !win.compact
            anchors { left: thumb.right; leftMargin: ui.px(14); verticalCenter: thumb.verticalCenter }
            width: Math.max(0, Math.min(ui.px(360), (strip.width - transport.width) / 2 - thumb.x - thumb.width - ui.px(34))) // clear of the controls
            Text {
                width: parent.width
                text: fs.preview ? fs.st.preview.title : (fs.st.title || "")
                color: ui.bright
                elide: Text.ElideRight
                font { family: ui.sans; pixelSize: ui.px(16); weight: Font.DemiBold }
            }
            Text {
                width: parent.width
                text: fs.preview ? fs.st.preview.artist : [fs.st.artist, fs.st.album].filter(x => x).join("  ·  ")
                color: ui.dim
                elide: Text.ElideRight
                font { family: ui.sans; pixelSize: ui.px(13) }
            }
        }
        Controls {
            id: transport
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: ui.px(12) }
            width: win.compact ? Math.max(0, strip.width - ui.px(24)) : Math.min(ui.px(560), strip.width - 2 * ui.px(200)) // clear of the buttons beside it
        }
        Row {
            visible: !win.compact
            anchors { right: parent.right; rightMargin: ui.gap; verticalCenter: thumb.verticalCenter }
            spacing: ui.px(2)
            Repeater {
                // In words, not icons: next to the player's buttons, a skip
                // or a loop would read as the music's. ‹ name › picks the
                // style, then whether it changes by itself, then out.
                model: [["‹", "", "shift+tab", "shift+tab"],
                        ["", store.vizNames[store.vizStyle] || "", "v", "v"],
                        ["›", "", "tab", "tab"],
                        ["", daemon.options.no_viz_cycle ? "stays" : "changes each minute", "a", "a"],
                        ["", daemon.options.viz_bare ? "controls hide" : "controls stay", "x", "x"],
                        ["✕", "", "f", "f"]]
                MorphButton {
                    required property var modelData
                    icon: modelData[0]
                    label: modelData[1]
                    key: modelData[2]
                    onClicked: store.dispatch(modelData[3], { text: modelData[3].length === 1 ? modelData[3] : "", modifiers: 0 })
                }
            }
        }
    }

    // v, or a click on the style's name: the styles unfold upward from the
    // strip, each with a picture of it, to pick from; moving through them
    // with the keys shows each.
    Item {
        id: styles
        z: 10
        visible: opacity > 0
        opacity: store.vizList ? 1 : 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 180 } }
        anchors { right: parent.right; rightMargin: ui.gap; bottom: strip.top; bottomMargin: -ui.px(10) }
        width: grid.width + ui.px(24)
        height: grid.height + ui.px(24)
        transform: Translate {
            y: store.vizList ? 0 : ui.px(14)
            Behavior on y { enabled: !ui.calm; NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        }
        TapHandler {} // a click between the pictures does not reach the stage

        Rectangle {
            anchors.fill: parent
            radius: ui.px(16)
            color: Qt.alpha(ui.deep, 0.92)
            border { width: 1; color: Qt.alpha(ui.here, 0.35) }
        }
        Grid {
            id: grid
            anchors.centerIn: parent
            readonly property int tile: ui.px(150)
            columns: Math.max(2, Math.min(4, Math.floor((fs.width - 2 * ui.gap - ui.px(24)) / (tile + ui.px(8)))))
            spacing: ui.px(8)
            Repeater {
                model: store.vizNames
                Item {
                    id: tile
                    required property int index
                    required property string modelData
                    readonly property bool picked: store.vizSel === index || tileHover.hovered
                    width: grid.tile
                    height: picture.height + ui.px(26)
                    Rectangle {
                        anchors { fill: parent; margins: -ui.px(4) }
                        radius: ui.px(10)
                        visible: tile.picked
                        color: Qt.alpha(ui.here, store.vizSel === tile.index ? 0.22 : 0.1)
                        border { width: 1; color: Qt.alpha(ui.here, 0.55) }
                    }
                    Image {
                        id: picture
                        width: parent.width
                        height: Math.round(width / 2)
                        source: Qt.resolvedUrl("viz/thumbs/" + tile.modelData + ".jpg")
                        sourceSize: Qt.size(320, 160)
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        opacity: tile.picked ? 1 : 0.8
                    }
                    Text {
                        anchors { left: parent.left; right: parent.right; top: picture.bottom; topMargin: ui.px(5) }
                        text: (tile.index < 10 ? (tile.index + 1) % 10 + "  " : "    ") + tile.modelData
                        color: tile.picked ? ui.bright : ui.fg
                        elide: Text.ElideRight
                        font { family: ui.sans; pixelSize: ui.px(12); weight: tile.picked ? Font.DemiBold : Font.Normal }
                    }
                    HoverHandler { id: tileHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: store.vizPick(tile.index) }
                }
            }
        }
    }
}
