// Controls are the transport, all glass: shuffle and repeat; back, the
// play orb, next; dislike, radio and the volume.
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

    // Dislike, radio and the volume, glass as the rest.
    Row {
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: ui.px(10)
        visible: !store.st.preview
        Orb {
            size: ui.px(34); icon: "󰔑"
            readonly property int mark: { store.ratingRev; return store.rating[store.st.id] || 0 }
            on: mark < 0
            onClicked: store.ratePlaying(-1)
        }
        Orb { size: ui.px(34); icon: "󰐹"; onClicked: store.radioPlaying() }
        // The volume: click mutes, the wheel turns it and says how loud.
        Orb {
            size: ui.px(34)
            icon: (store.st.volume || 0) > 0 ? "󰕾" : "󰖁"
            onClicked: store.key("m")
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.NoButton
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

}
