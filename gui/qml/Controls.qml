// Controls are the transport, all glass: shuffle and repeat, back, the
// play orb, next, and the volume.
import QtQuick

Item {
    id: ctl
    height: play.height

    Row {
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        spacing: ui.px(10)
        Orb { size: ui.px(34); icon: "󰒝"; on: !!store.st.shuffle; onClicked: store.key("s") }
        Orb { size: ui.px(34); icon: store.st.repeat === 1 ? "󰑘" : "󰑖"; on: (store.st.repeat || 0) > 0; onClicked: store.key("r") }
    }

    Row {
        anchors.centerIn: parent
        spacing: ui.px(18)
        Orb { anchors.verticalCenter: parent.verticalCenter; size: ui.px(46); icon: "󰒮"; onClicked: store.key("p") }
        Orb { id: play; size: ui.px(66); lit: true; icon: store.st.playing ? "󰏤" : "󰐊"; onClicked: store.toggle() }
        Orb { anchors.verticalCenter: parent.verticalCenter; size: ui.px(46); icon: "󰒭"; onClicked: store.key("n") }
    }

    // The volume, a pill of glass: click mutes, the wheel turns it.
    Rectangle {
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        width: vol.implicitWidth + ui.px(22)
        height: ui.px(34)
        radius: height / 2
        color: ui.effects ? "transparent" : Qt.alpha(ui.bright, 0.08) // without effects, plain
        GlassShape { anchors.fill: parent; visible: ui.effects; hot: volArea.containsMouse ? 1 : 0 }
        Row {
            id: vol
            anchors.centerIn: parent
            spacing: ui.px(7)
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: (store.st.volume || 0) > 0 ? "󰕾" : "󰖁"
                color: ui.fg
                font { family: ui.mono; pixelSize: ui.px(15) }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Math.round((store.st.volume || 0) * 100)
                color: ui.fg
                font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold; features: { "tnum": 1 } }
            }
        }
        MouseArea {
            id: volArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: store.key("m")
            // Each notch turns it 5%; a touchpad's small steps add up.
            // Up is louder, however the scrolling is set to feel.
            property real acc: 0
            onWheel: w => {
                const dy = w.inverted ? -w.angleDelta.y : w.angleDelta.y
                acc += dy
                while (Math.abs(acc) >= 120) {
                    store.setVolume((store.st.volume || 0) + (acc > 0 ? 0.05 : -0.05))
                    acc -= acc > 0 ? 120 : -120
                }
            }
        }
    }
}
