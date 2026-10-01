// FullStage is f: the visualizer over the whole window, its twelve styles
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

    // What plays, in a strip along the bottom.
    Rectangle {
        id: strip
        z: 5
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: ui.px(92)
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.6; color: Qt.alpha(ui.deep, 0.75) }
            GradientStop { position: 1; color: Qt.alpha(ui.deep, 0.9) }
        }
        Image {
            id: thumb
            anchors { left: parent.left; leftMargin: ui.gap; bottom: parent.bottom; bottomMargin: ui.px(18) }
            width: ui.px(48)
            height: width
            source: win.artwork ? "image://cover/" + encodeURIComponent(win.artwork) : ""
            sourceSize: Qt.size(ui.px(96), ui.px(96))
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
        Column {
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
            width: Math.min(ui.px(560), strip.width - 2 * ui.px(200)) // clear of the buttons beside it
        }
        Row {
            anchors { right: parent.right; rightMargin: ui.gap; verticalCenter: thumb.verticalCenter }
            spacing: ui.px(2)
            Repeater {
                model: [["󰒭", "next style", "tab", "tab"], ["󰕰", "styles", "v", "v"],
                        [daemon.options.no_viz_cycle ? "󰐎" : "󰑖", daemon.options.no_viz_cycle ? "style stays" : "changes each minute", "a", "a"],
                        ["󰊔", "close", "f", "f"]]
                MorphButton {
                    required property var modelData
                    icon: modelData[0]
                    label: modelData[1]
                    quiet: true
                    key: modelData[2]
                    onClicked: store.dispatch(modelData[3], { text: modelData[3].length === 1 ? modelData[3] : "", modifiers: 0 })
                }
            }
        }
    }

    // v: the styles, as a list to pick from; moving through it shows each.
    Box {
        visible: store.vizList
        z: 10
        title: "styles"
        boxWidth: ui.px(320)
        Repeater {
            model: store.vizNames
            Item {
                required property int index
                required property string modelData
                width: parent.width
                height: ui.px(26)
                Rectangle {
                    anchors { fill: parent; leftMargin: -ui.px(10); rightMargin: -ui.px(10) }
                    radius: ui.px(8)
                    visible: store.vizSel === index
                    color: Qt.alpha(ui.here, 0.2)
                    border { width: 1; color: Qt.alpha(ui.here, 0.5) }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: (index < 10 ? (index + 1) % 10 + "   " : "     ") + modelData
                    color: store.vizSel === index ? ui.bright : ui.fg
                    font { family: ui.sans; pixelSize: ui.px(14) }
                }
                TapHandler { onTapped: store.vizPick(index) }
            }
        }
    }
}
