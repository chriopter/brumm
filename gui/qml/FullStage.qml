// One renderer for the player background and the large visualizer.
// Coverwall, Coverflow and Waves share the same stage as the audio styles.
// In the player view, the main animation fades out before the controls.
import QtQuick
import QtQuick.Window
import QtQuick.Controls
import Brumm

Item {
    id: fs

    readonly property var st: store.st
    readonly property bool preview: !!st.preview
    property bool embedded: false
    property Item settingsAnchor: null
    function openSettings(anchor) {
        settingsAnchor = anchor || null
        store.vizSel = store.vizStyle
        store.vizFrom = store.vizStyle
        store.vizList = true
    }
    readonly property bool shown: store.vizActive
    readonly property color backgroundColor: {
        const current = onA ? slotA.item : slotB.item
        return current ? current.backgroundColor : ui.deep
    }
    clip: true
    // Its styles are shaders: without them (the software renderer) there
    // is nothing to draw, nothing to hear. Minimized, the daemon need not
    // send the sound either.
    readonly property bool artworkStyle: ["coverwall", "waves", "coverflow"].includes(store.vizNames[store.vizDisplayed])
    readonly property bool moving: shown && ui.effects && win.visibility !== Window.Minimized
        && (store.full || !!store.st.playing)
        && !win.popup && !store.menuOpen
    readonly property bool listening: moving && !artworkStyle
    onListeningChanged: daemon.listen(listening)
    MappedRect {
        id: playerBounds
        sourceItem: playbackStage.visualizerArea
        targetItem: fs
    }
    readonly property rect coverArea: embedded ? playerBounds.bounds
        : Qt.rect(0, 0, width, Math.max(0, height - strip.height))
    readonly property real coverSize: embedded ? playbackStage.coverSize : Math.min(width * 0.38, (height - strip.height) * 0.6, ui.px(650))

    // The textures every style reads (see viz/VizEffect.qml), fresh each
    // tick, written over in place.
    VizTexture { id: specTex; audio: vizAudio }
    VizTexture { id: waveTex; audio: vizAudio; wave: true }

    // The listener moves on once a frame while the visualizer shows.
    Binding { target: vizClock; property: "running"; value: fs.moving }
    Binding { target: vizClock; property: "fps"; value: fs.artworkStyle && fs.embedded ? 20 : store.vizRate(vizClock.refreshRate) }
    // The visualizer's rate must never throttle resizing or list scrolling.
    Binding { target: vizClock; property: "limitFrames"; value: !fs.embedded }
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
        // Every style owns the whole window; only its main subject uses sceneArea.
        anchors.fill: parent
        readonly property rect sceneArea: fs.coverArea
        readonly property real sceneCoverSize: fs.coverSize
        active: fs.shown && ui.effects && style >= 0 && (front || opacity > 0)
        // On a large or dense screen the styles are drawn with about three
        // million pixels and scaled up: they are soft light, and four times
        // the pixels would cost four times the GPU.
        readonly property real shrink: Math.min(1, Math.sqrt(3.0e6 / Math.max(1, width * height * Screen.devicePixelRatio * Screen.devicePixelRatio)))
        RenderSurface { id: surface; width: slot.width; height: slot.height; density: Screen.devicePixelRatio * slot.shrink }
        layer.enabled: ui.effects && shrink < 1
        layer.smooth: true
        layer.textureSize: surface.textureSize
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
        function onVizStyleChanged() { fs.show(store.vizDisplayed, !store.vizQuick) }
        function onVizHoverChanged() { fs.show(store.vizDisplayed, false) }
        function onVizActiveChanged() { if (store.vizActive) fs.show(store.vizStyle, false) }
    }
    Component.onCompleted: show(store.vizStyle, false)

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
        visible: !fs.embedded
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: ui.px(fs.embedded ? 16 : 40) }
        z: 5
        text: store.vizNames[store.vizDisplayed] || ""
        color: ui.bright
        opacity: 0
        font { family: ui.sans; pixelSize: ui.px(fs.embedded ? 20 : 34); weight: Font.Light; letterSpacing: fs.embedded ? 3 : 8 }
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
        running: fs.moving && store.full && !daemon.options.no_viz_cycle && !store.vizList
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
        visible: !fs.embedded && opacity > 0
        parent: fs.embedded ? playbackStage.visualizerArea : fs
        z: 5
        opacity: !fs.embedded && fs.bare ? 0 : 1
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 150; easing.type: Easing.InOutQuad } }
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: ui.px(fs.embedded ? 48 : 92)
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.6; color: fs.embedded ? "transparent" : Qt.alpha(ui.deep, 0.75) }
            GradientStop { position: 1; color: fs.embedded ? "transparent" : Qt.alpha(ui.deep, 0.9) }
        }
        Image {
            id: thumb
            visible: !fs.embedded && !win.compact
            anchors { left: parent.left; leftMargin: ui.gap; bottom: parent.bottom; bottomMargin: ui.px(18) }
            width: ui.px(48)
            height: width
            source: win.artwork ? "image://cover/" + encodeURIComponent(win.artwork) : ""
            sourceSize: Qt.size(ui.px(96), ui.px(96))
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
        Column {
            visible: !fs.embedded && !win.compact
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
            visible: !fs.embedded
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: ui.px(12) }
            width: win.compact ? Math.max(0, strip.width - ui.px(24)) : Math.max(0, Math.min(ui.px(560), strip.width - 2 * (styleControls.width + ui.gap + ui.px(12)))) // clear of the buttons beside it
        }
        Orb {
            id: styleControls
            visible: !fs.embedded
            anchors { right: parent.right; rightMargin: ui.gap; verticalCenter: parent.verticalCenter }
            size: ui.px(34)
            icon: "󰀻"
            tooltip: "Visualizer · " + (store.vizNames[store.vizDisplayed] || "")
            on: settingsMenu.opened
            onClicked: fs.openSettings(styleControls)
        }
    }

    component PanelButton: Button {
        implicitWidth: contentItem.implicitWidth + ui.px(24)
        implicitHeight: ui.px(34)
        padding: ui.px(10)
        contentItem: Text {
            text: parent.text
            color: parent.checked ? ui.here : ui.bright
            font { family: ui.sans; pixelSize: ui.px(12) }
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: ui.px(8)
            color: "transparent"
            GlassShape {
                anchors.fill: parent
                radius: ui.px(8)
                hot: parent.parent.hovered || parent.parent.checked ? 1 : 0
            }
        }
    }

    Connections {
        target: store
        function onVizListChanged() {
            if (store.vizList) settingsMenu.open()
            else settingsMenu.close()
        }
    }
    MappedRect {
        id: settingsBounds
        sourceItem: settingsMenu.origin
        targetItem: win.contentItem
    }
    Popup {
        id: settingsMenu
        parent: win.contentItem
        // Keyboard opens use the current view's button, never a stale anchor.
        readonly property Item origin: fs.settingsAnchor && fs.settingsAnchor.visible
            ? fs.settingsAnchor : !fs.embedded ? styleControls
            : win.compact ? compactControls.visualizerButton : playbackStage.visualizerButton
        readonly property rect at: settingsBounds.bounds
        width: Math.min(ui.px(330), win.width - ui.px(32))
        height: Math.min(implicitHeight, win.height - ui.px(32))
        x: Math.max(ui.px(16), Math.min(at.x + at.width - width, win.width - width - ui.px(16)))
        y: Math.max(ui.px(16), Math.min(at.y - height - ui.px(8), win.height - height - ui.px(16)))
        padding: ui.px(12)
        focus: true
        onOpened: { fs.stir(); panelFlick.forceActiveFocus(); Qt.callLater(() => previews.showSelection()) }
        onClosed: { fs.settingsAnchor = null; store.vizHover = -1; store.vizList = false; win.focusPlayer() }
        background: Rectangle {
            color: Qt.alpha(ui.deep, 0.97)
            radius: ui.px(16)
            border.color: Qt.alpha(ui.bright, 0.18)
        }
        contentItem: Flickable {
            id: panelFlick
            clip: true
            implicitHeight: panel.implicitHeight
            contentHeight: panel.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Keys.onPressed: event => {
                const key = win.keyName(event)
                store.vizHover = -1
                if (key && store.vizKey(key)) event.accepted = true
            }
            Column {
                id: panel
                width: parent.width
                spacing: ui.px(10)
                Row {
                    width: parent.width
                    Text {
                        width: parent.width - done.width
                        height: done.height
                        text: "Visualizer · " + (store.vizNames[store.vizDisplayed] || "")
                        color: ui.bright
                        verticalAlignment: Text.AlignVCenter
                        font { family: ui.sans; pixelSize: ui.px(13); weight: Font.DemiBold }
                    }
                    Orb { id: done; size: ui.px(26); icon: "󰅖"; tooltip: "Close"; onClicked: store.vizList = false }
                }
                Flickable {
                    id: previews
                    width: parent.width
                    height: Math.min(grid.height, 3 * (grid.tile + ui.px(26)) + 2 * grid.spacing)
                    contentWidth: width
                    contentHeight: grid.height
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    function showSelection() {
                        const top = Math.floor(store.vizSel / grid.columns) * (grid.tile + ui.px(26) + grid.spacing)
                        if (top < contentY) contentY = top
                        else if (top + grid.tile + ui.px(26) > contentY + height)
                            contentY = top + grid.tile + ui.px(26) - height
                    }
                    Connections { target: store; function onVizSelChanged() { previews.showSelection() } }
        Grid {
            id: grid
            width: previews.width - ui.px(10)
            columns: 3
            readonly property real tile: (width - (columns - 1) * spacing) / columns
            spacing: ui.px(8)
            Repeater {
                model: settingsMenu.visible ? store.vizNames : []
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
                        visible: !["coverwall", "coverflow"].includes(tile.modelData)
                        width: parent.width
                        height: width
                        source: tile.modelData === "waves" ? "" : ["coverwall", "coverflow"].includes(tile.modelData) ? (win.artwork ? "image://cover/" + encodeURIComponent(win.artwork) : "") : Qt.resolvedUrl("viz/thumbs/" + tile.modelData + ".jpg")
                        sourceSize: Qt.size(320, 160)
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        opacity: tile.picked ? 1 : 0.8
                    }
                    Loader {
                        anchors.fill: picture
                        active: ["coverwall", "coverflow", "waves"].includes(tile.modelData)
                        sourceComponent: tile.modelData === "waves" ? wavePreview : artworkPreview
                        Component { id: wavePreview; Backdrop {} }
                        Component { id: artworkPreview; ArtworkPreview { flow: tile.modelData === "coverflow" } }
                    }
                    Text {
                        anchors { left: parent.left; right: parent.right; top: picture.bottom; topMargin: ui.px(5) }
                        text: (tile.index < 10 ? (tile.index + 1) % 10 + "  " : "    ") + tile.modelData
                        color: tile.picked ? ui.bright : ui.fg
                        elide: Text.ElideRight
                        font { family: ui.sans; pixelSize: ui.px(12); weight: tile.picked ? Font.DemiBold : Font.Normal }
                    }
                    HoverHandler {
                        id: tileHover
                        cursorShape: Qt.PointingHandCursor
                        onHoveredChanged: {
                            if (hovered) store.vizHover = tile.index
                            else if (store.vizHover === tile.index) store.vizHover = -1
                        }
                    }
                    TapHandler { onTapped: { store.vizSel = tile.index; store.vizShow(tile.index, true) } }
                }
            }
        }
                }

                Rectangle { width: parent.width; height: 1; color: ui.edge }
                Row {
                    width: parent.width
                    spacing: ui.px(6)
                    PanelButton {
                        id: rateButton
                        text: store.vizRateLabel(vizClock.refreshRate) + " ▾"
                        onClicked: rateMenu.open()
                    }
                    Orb {
                        id: cycleButton
                        visible: store.full
                        size: ui.px(30)
                        icon: "󰑖"
                        on: !daemon.options.no_viz_cycle
                        tooltip: "Auto-change styles"
                        onClicked: cycleMenu.open()
                    }
                    Orb {
                        id: hideButton
                        visible: store.full
                        size: ui.px(30)
                        icon: "󰈈"
                        on: !!daemon.options.viz_bare
                        tooltip: "Controls visibility"
                        onClicked: hideMenu.open()
                    }
                    Orb {
                        size: ui.px(30)
                        icon: fs.embedded ? "↗" : "↙"
                        tooltip: fs.embedded ? "Large view · F" : "Player view · F"
                        onClicked: { store.vizList = false; store.cycleVisualizer() }
                    }
                    Orb {
                        size: ui.px(30)
                        icon: "⛶"
                        tooltip: win.visibility === Window.FullScreen ? "Leave fullscreen · Shift+F" : "Fullscreen · Shift+F"
                        onClicked: { store.vizList = false; win.toggleVizFullscreen() }
                    }
                }
            }
        }
    }
    component SettingMenu: Menu {
        y: -height - ui.px(6)
        width: ui.px(180)
        padding: ui.px(6)
        background: Rectangle { radius: ui.px(10); color: Qt.alpha(ui.deep, 0.98); border.color: ui.edge }
    }
    SettingMenu {
        id: rateMenu
        parent: rateButton
        Repeater {
            model: [{value:0, label:"30 FPS"}, {value:60, label:"60 FPS"}, {value:120, label:"120 FPS"}, {value:-1, label:"Auto"}]
            ContextAction {
                required property var modelData
                text: ((daemon.options.viz_fps || 0) === modelData.value ? "✓  " : "    ") + modelData.label
                onTriggered: daemon.setOption("viz_fps", modelData.value)
            }
        }
    }
    SettingMenu {
        id: cycleMenu
        parent: cycleButton
        ContextAction { text: "Off"; onTriggered: daemon.setOption("no_viz_cycle", true) }
        ContextAction { text: "Every minute"; onTriggered: daemon.setOption("no_viz_cycle", false) }
    }
    SettingMenu {
        id: hideMenu
        parent: hideButton
        ContextAction { text: "Always visible"; onTriggered: daemon.setOption("viz_bare", false) }
        ContextAction { text: "Hide when idle"; onTriggered: daemon.setOption("viz_bare", true) }
    }

}
