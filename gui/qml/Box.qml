// Box is a popup: a card of glass in the middle of the window, over the
// rest blurred and dimmed (Main.qml), lit along its top and bottom edges.
import QtQuick
import QtQuick.Effects

Item {
    id: box
    property string title
    default property alias content: inner.data
    property int boxWidth: ui.px(480)

    anchors.centerIn: parent
    width: Math.min(boxWidth + ui.px(56), parent.width - ui.gap * 2)
    height: inner.implicitHeight + ui.px(92)

    TapHandler {} // a click on the card does not close it

    RectangularShadow {
        visible: ui.effects
        anchors.fill: parent
        radius: card.radius
        y: ui.px(16)
        blur: ui.px(48)
        color: Qt.alpha("black", 0.55)
    }
    RectangularShadow { // a halo in the color of what plays
        visible: ui.effects
        anchors.fill: parent
        radius: card.radius
        blur: ui.px(30)
        spread: -ui.px(8)
        color: Qt.alpha(ui.here, 0.25)
    }
    Rectangle {
        id: card
        anchors.fill: parent
        radius: ui.px(20)
        gradient: Gradient {
            GradientStop { position: 0; color: ui.tint(ui.deep, 0.16) }
            GradientStop { position: 1; color: ui.tint(ui.deep, 0.06) }
        }
        border { width: 1; color: Qt.alpha(ui.bright, 0.14) }
        Rectangle { // the gloss
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 1 }
            height: ui.px(56)
            radius: parent.radius
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.alpha("white", 0.07) }
                GradientStop { position: 1; color: "transparent" }
            }
        }
    }
    // Held by a line of light along its top and its bottom, as the XMB
    // holds its dialogs.
    Beam { anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: card.radius; rightMargin: card.radius } }
    Beam {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: card.radius; rightMargin: card.radius }
        strength: 0.6
    }

    Text {
        id: heading
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: ui.px(24) }
        text: box.title.toUpperCase()
        color: ui.here
        font { family: ui.sans; pixelSize: ui.px(11); weight: Font.Bold; letterSpacing: 1.8 }
    }
    Column {
        id: inner
        anchors { horizontalCenter: parent.horizontalCenter; top: heading.bottom; topMargin: ui.px(18) }
        width: box.width - ui.px(56)
        spacing: ui.px(6)
    }
}
