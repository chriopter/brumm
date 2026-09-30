// Orb is a round button of glass (GlassShape). Lit, it is filled with the
// color of what plays and glows — the play button; on, it glows softly.
import QtQuick
import QtQuick.Effects

Item {
    id: orb
    property string icon
    property int size: ui.px(42)
    property bool lit: false
    property bool on: false
    signal clicked()

    width: size
    height: size
    scale: tap.pressed ? 0.93 : hover.hovered ? 1.05 : 1
    Behavior on scale { enabled: !ui.calm; NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }

    RectangularShadow { // the glow
        visible: ui.effects && (orb.lit || orb.on || hover.hovered)
        anchors.fill: parent
        radius: width / 2
        blur: orb.size * (orb.lit ? 0.4 : 0.3)
        color: Qt.alpha(ui.here, orb.lit ? 0.6 : 0.35)
    }
    GlassShape {
        visible: ui.effects
        anchors.fill: parent
        lit: orb.lit ? 1 : 0
        hot: hover.hovered || orb.on ? 1 : 0
    }
    Rectangle { // without effects, plain
        visible: !ui.effects
        anchors.fill: parent
        radius: width / 2
        color: orb.lit ? ui.here : Qt.alpha(ui.bright, 0.1)
        border { width: 1; color: Qt.alpha(ui.bright, 0.25) }
    }
    Text {
        anchors.centerIn: parent
        anchors.horizontalCenterOffset: orb.icon === "󰐊" ? orb.size * 0.04 : 0 // the triangle's weight sits left
        text: orb.icon
        color: orb.lit ? (theme.dark ? ui.deep : "white") : orb.on ? ui.bright : hover.hovered ? ui.bright : ui.fg
        font { family: ui.mono; pixelSize: orb.size * 0.44 }
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: tap; onTapped: orb.clicked() }
}
