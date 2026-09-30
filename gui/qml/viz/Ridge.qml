// Ridge is the terminal's Unknown Pleasures: the spectrum frozen into a
// ridgeline ten times a second, the lines receding toward a horizon, each
// hiding what lies behind it. The history lives in a small texture that
// draws over itself (shaders/viz_ridge_push.frag); one full pass shows it
// (shaders/viz_ridge.frag).
import QtQuick

VizEffect {
    id: fx

    // Ten pushes a second; slide is how far the lines have gone toward the
    // next one, so they recede smoothly between pushes.
    property real slide: 0
    FrameAnimation {
        running: fx.running
        onTriggered: {
            const s = fx.slide + Math.min(frameTime, 0.1) * 10
            push.shift = s >= 1 ? 1 : 0
            if (push.shift)
                push.seed = Math.random() * 97
            fx.slide = s % 1
            history.scheduleUpdate()
        }
    }

    // The history: 128 points by the live line and the forty behind it.
    ShaderEffect {
        id: push
        width: 128
        height: 41
        property real shift: 0
        property real seed: 0
        property var prev: history
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_ridge_push.frag.qsb"
    }
    ShaderEffectSource {
        id: history
        visible: false
        sourceItem: push
        hideSource: true
        live: false
        recursive: true
        smooth: true
        textureSize: Qt.size(128, 41)
    }

    ShaderEffect {
        anchors.fill: parent
        property real slide: fx.slide
        property real beat: fx.audio.beat
        property real bass: fx.audio.bass
        property real level: fx.audio.level
        property real aspect: width / Math.max(1, height)
        property color tint: fx.here
        property var hist: history
        fragmentShader: "qrc:/shaders/viz_ridge.frag.qsb"
    }
}
