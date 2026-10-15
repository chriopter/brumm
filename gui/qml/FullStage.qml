// FullStage is f: the visualizer over the whole window, its thirteen styles
// named and ordered as the terminal player's, dissolving from one into the
// next every minute (a: stays), with what plays in a strip along the
// bottom. It listens to the daemon's sound only while it shows.
import QtQuick
import QtQuick.Window
import QtQuick.Controls
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
    Binding { target: vizClock; property: "running"; value: fs.moving }
    Binding { target: vizClock; property: "fps"; value: store.vizRate(vizClock.refreshRate) }
    VizFrame {
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
        function onVizStyleChanged() { if (minutely.running) minutely.restart() }
    }

    // x: the controls fade out while the mouse rests, and come back as
    // soon as it moves; with the list of styles open they stay.
    property bool resting: false
    readonly property bool autoHide: !!daemon.options.viz_bare
    onAutoHideChanged: stir()
    readonly property bool bare: autoHide && resting && !store.vizList && !settingsMenu.opened
    Timer { id: rest; interval: 1000; running: fs.shown; onTriggered: fs.resting = true }
    function stir() { resting = false; if (shown) rest.restart(); else rest.stop() }
    onShownChanged: stir()
    HoverHandler {
        property point lastPosition: Qt.point(-1, -1)
        cursorShape: fs.bare ? Qt.BlankCursor : Qt.ArrowCursor
        onPointChanged: {
            const p = point.position
            if (p.x === lastPosition.x && p.y === lastPosition.y) return
            lastPosition = p
            fs.stir()
        }
    }

    // What plays, in a strip along the bottom.
    Rectangle {
        id: strip
        z: 5
        opacity: fs.bare ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }
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
            width: win.compact ? Math.max(0, strip.width - ui.px(24)) : Math.max(0, Math.min(ui.px(560), strip.width - 2 * (styleControls.width + ui.gap + ui.px(12)))) // clear of the buttons beside it
        }
        Row {
            id: styleControls
            visible: !win.compact
            anchors { right: parent.right; rightMargin: ui.gap; verticalCenter: thumb.verticalCenter }
            spacing: ui.px(7)
            Repeater {
                model: [
                    {icon:"󰀻", tip:(store.vizNames[store.vizStyle] || "Visualizer") + " · Choose style (V)", action:"v", active:store.vizList},
                    {icon:"󰒓", tip:"Visualizer settings · " + store.vizRateLabel(vizClock.refreshRate), action:"settings", active:settingsMenu.opened},
                    {icon:"󰅖", tip:"Exit visualizer · F", action:"f", active:false}
                ]
                Item {
                    required property var modelData
                    required property int index
                    width: button.width
                    height: button.height
                    Orb {
                        id: button
                        size: ui.px(34)
                        icon: modelData.icon
                        tooltip: modelData.tip
                        on: modelData.active
                        onClicked: {
                            if (modelData.action === "settings") { store.vizList = false; settingsMenu.open() }
                            else store.dispatch(modelData.action, { text: modelData.action.length === 1 ? modelData.action : "", modifiers: 0 })
                        }
                    }
                }
            }
        }
    }

    Popup {
        id: settingsMenu
        x: fs.width - width - ui.gap
        y: fs.height - strip.height - height
        width: ui.px(240)
        padding: ui.px(8)
        focus: true
        onOpened: fs.stir()
        background: Rectangle {
            color: Qt.alpha(ui.deep, 0.96)
            radius: ui.px(12)
            border.color: Qt.alpha(ui.bright, 0.18)
        }
        contentItem: Column {
            Text {
                text: "QUALITY · FPS"
                color: ui.dim
                height: ui.px(30)
                verticalAlignment: Text.AlignVCenter
                font { family: ui.sans; pixelSize: ui.px(11); weight: Font.DemiBold }
            }
            Repeater {
                model: [{value:0,label:"30 FPS"}, {value:60,label:"60 FPS"}, {value:120,label:"120 FPS"},
                        {value:-1,label:"Auto · " + Math.round(vizClock.refreshRate) + " FPS"}]
                Button {
                    required property var modelData
                    width: parent.width
                    height: ui.px(36)
                    readonly property bool selected: (daemon.options.viz_fps || 0) === modelData.value
                    contentItem: Text {
                        text: (parent.selected ? "●  " : "○  ") + parent.modelData.label
                        color: parent.selected ? ui.here : ui.bright
                        verticalAlignment: Text.AlignVCenter
                        font { family: ui.sans; pixelSize: ui.px(13) }
                    }
                    background: Rectangle { radius: ui.px(6); color: parent.hovered ? ui.glassHi : "transparent" }
                    onClicked: { daemon.setOption("viz_fps", modelData.value); fs.stir() }
                }
            }
            Rectangle { width: parent.width; height: 1; color: ui.edge }
            Repeater {
                model: [
                    {label:"Auto-change styles",key:"no_viz_cycle",on:!daemon.options.no_viz_cycle},
                    {label:"Auto-hide controls",key:"viz_bare",on:!!daemon.options.viz_bare}
                ]
                Button {
                    required property var modelData
                    width: parent.width
                    height: ui.px(38)
                    contentItem: Text {
                        text: (parent.modelData.on ? "☑  " : "☐  ") + parent.modelData.label
                        color: parent.modelData.on ? ui.here : ui.bright
                        verticalAlignment: Text.AlignVCenter
                        font { family: ui.sans; pixelSize: ui.px(13) }
                    }
                    background: Rectangle { radius: ui.px(6); color: parent.hovered ? ui.glassHi : "transparent" }
                    onClicked: { daemon.setOption(modelData.key, !daemon.options[modelData.key]); fs.stir() }
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
