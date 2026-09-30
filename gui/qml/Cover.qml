// Cover is album art on a stand: rounded, with a soft shadow and, if
// asked, its reflection on the floor below. A new cover fades in over the
// old one. The shadow and reflection are drawn again only when the cover
// changes.
import QtQuick
import QtQuick.Effects

Item {
    id: c
    property string url: ""
    property int maxSource: 600
    property bool reflect: false
    property real reflection: 0.28 // the reflection's height, as a share of the cover's
    property int radius: ui.coverRadius
    implicitHeight: width * (reflect ? 1 + reflection : 1)

    // The art itself, two images taking turns, drawn off screen once.
    Item {
        id: art
        width: c.width
        height: c.width
        visible: false
        layer.enabled: true

        Rectangle {
            anchors.fill: parent
            color: ui.glassHi
            Text {
                anchors.centerIn: parent
                text: "󰀥"
                color: ui.edge
                font { family: ui.mono; pixelSize: Math.max(ui.px(24), c.width / 5) }
            }
        }
        // Two images take turns: the new cover loads behind and fades in
        // over the one it replaces.
        property bool onA: true
        Image { id: a; anchors.fill: parent; asynchronous: true; fillMode: Image.PreserveAspectCrop; smooth: true; mipmap: true
            sourceSize: Qt.size(c.maxSource, c.maxSource); z: art.onA ? 1 : 0
            // In front: shown once loaded. Behind: shown until the front is.
            opacity: art.onA ? (status === Image.Ready ? 1 : 0) : (b.status === Image.Ready ? 0 : 1)
            Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 320; easing.type: Easing.InOutCubic } } }
        Image { id: b; anchors.fill: parent; asynchronous: true; fillMode: Image.PreserveAspectCrop; smooth: true; mipmap: true
            sourceSize: Qt.size(c.maxSource, c.maxSource); z: art.onA ? 0 : 1
            opacity: !art.onA ? (status === Image.Ready ? 1 : 0) : (a.status === Image.Ready ? 0 : 1)
            Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 320; easing.type: Easing.InOutCubic } } }
    }

    Rectangle {
        id: mask
        width: c.width
        height: c.width
        radius: c.radius
        visible: false
        layer.enabled: true
    }

    // The shadow: drawn from its shape, no image of it made. On a floor
    // that reflects it, none.
    RectangularShadow {
        visible: ui.effects && !c.reflect && (a.status === Image.Ready || b.status === Image.Ready)
        width: c.width
        height: c.width
        y: ui.px(12)
        radius: c.radius
        blur: ui.px(28)
        spread: -ui.px(4)
        color: ui.shade
    }

    MultiEffect { // the art, its corners rounded
        visible: ui.effects
        source: art
        width: c.width
        height: c.width
        autoPaddingEnabled: false
        maskEnabled: true
        maskSource: mask
    }
    ShaderEffectSource { // without effects, the art as it is
        visible: !ui.effects
        sourceItem: art
        width: c.width
        height: c.width
    }

    // The floor's reflection: the art upside down, fading out.
    ShaderEffect {
        visible: c.reflect && ui.effects
        y: c.width + ui.px(4)
        width: c.width
        height: c.width * c.reflection
        property var source: ShaderEffectSource {
            sourceItem: art
            sourceRect: Qt.rect(0, c.width * (1 - c.reflection), c.width, c.width * c.reflection)
            textureSize: Qt.size(Math.max(1, Math.round(c.width / 2)), Math.max(1, Math.round(c.width * c.reflection / 2)))
        }
        property real strength: theme.dark ? 0.22 : 0.35
        fragmentShader: "qrc:/shaders/reflect.frag.qsb"
    }

    onUrlChanged: {
        const src = url ? "image://cover/" + encodeURIComponent(url) : ""
        const front = art.onA ? a : b
        if (front.source.toString() === src) return
        art.onA = !art.onA
        ;(art.onA ? a : b).source = src
    }
    Component.onCompleted: urlChanged()
}
