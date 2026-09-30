// Backdrop is the window's ground: a deep gradient and the ribbons of
// light across it (shaders/backdrop.frag). They drift only while music
// plays and the window has focus — in the background, or with motion
// reduced, nothing is drawn at all — and brighten on the big stage.
import QtQuick

ShaderEffect {
    id: bd
    property real time: 0
    readonly property real aspect: width / Math.max(1, height)
    property real glowing: store.full ? 1.5 : (store.st.playing ? 1 : 0.6)
    property color topColor: ui.tint(ui.deep, 0.10)
    property color bottomColor: ui.tint(ui.deep, 0.22)
    property color glow: ui.here
    fragmentShader: "qrc:/shaders/backdrop.frag.qsb"

    Behavior on glowing { enabled: !ui.calm; NumberAnimation { duration: 900; easing.type: Easing.InOutQuad } }
    Behavior on topColor { enabled: !ui.calm; ColorAnimation { duration: 700 } }
    Behavior on bottomColor { enabled: !ui.calm; ColorAnimation { duration: 700 } }

    // It drifts with the window's clock (ui.clock), 20 frames a second:
    // the drift is slow, more would look the same.
    Connections {
        target: ui
        enabled: bd.visible
        function onClockChanged() { bd.time = ui.clock / 1000 }
    }
}
