// Equalizer is three bars that bob while a song plays, at a few frames a
// second like the terminal player's, and rest when it is paused, the
// window is in the background, or motion is reduced.
import QtQuick

Row {
    id: eq
    property bool running: false
    property color color: ui.here
    property var levels: [0.4, 0.7, 0.5]
    spacing: ui.px(2)
    height: ui.px(12)

    // It bobs with the window's clock (ui.clock), eight times a second,
    // in the frames the rest moves in.
    property int step: -1
    Connections {
        target: ui
        enabled: eq.running && eq.visible && !win.popup // under a popup's blur it would look the same
        function onClockChanged() {
            const s = Math.floor(ui.clock / 125)
            if (s === eq.step) return
            eq.step = s
            eq.levels = eq.levels.map(l => Math.max(0.2, Math.min(1, l + (Math.random() - 0.5) * 0.7)))
        }
    }

    Repeater {
        model: 3
        Rectangle {
            required property int index
            width: ui.px(3)
            height: Math.round(eq.height * (eq.running ? eq.levels[index] : 0.4))
            anchors.bottom: parent.bottom
            radius: width / 2
            color: Qt.alpha(eq.color, 0.85)
            Behavior on height { enabled: !ui.calm; NumberAnimation { duration: 180; easing.type: Easing.InOutSine } }
        }
    }
}
