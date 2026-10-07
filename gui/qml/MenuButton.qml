// MenuButton opens the one menu (store.menuRows): what there is to do,
// each with its key, and the shared options as switches — a glass button,
// and a card of glass hanging from it, lit along its top edge.
import QtQuick
import QtQuick.Effects

Item {
    id: f
    property string hint: "" // the option pointed at: what it does
    width: button.width
    height: button.height

    Rectangle {
        id: button
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        width: ui.px(38)
        height: ui.px(34)
        radius: ui.px(10)
        color: ui.effects ? "transparent" : Qt.alpha(ui.bright, 0.08) // without effects, plain
        GlassShape { anchors.fill: parent; visible: ui.effects; radius: ui.px(10); lit: 0; hot: hover.hovered || store.menuOpen ? 1 : 0 }
        Text {
            anchors.centerIn: parent
            text: "󰍜"
            color: store.menuOpen ? ui.bright : ui.fg
            font { family: ui.mono; pixelSize: ui.px(18) }
        }
        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: store.menuOpen ? store.closeMenu() : store.openMenu(false) }
    }

    // The menu, a card of glass hanging from the button. It lives on the
    // window itself, over everything: hung on the narrow sidebar, the clicks
    // beside the sidebar would not reach it.
    Item {
        id: menu
        parent: win.contentItem
        visible: store.menuOpen && f.visible
        readonly property point at: { store.menuOpen; win.width; win.height; return f.mapToItem(win.contentItem, 0, 0) }
        x: at.x
        y: at.y + button.height + ui.px(10)
        width: ui.px(300)
        height: list.implicitHeight + ui.px(40) // and a line for the hint
        z: 50

        TapHandler {} // a click between the rows does not close it

        RectangularShadow {
            visible: ui.effects
            anchors.fill: parent
            radius: card.radius
            y: ui.px(10)
            blur: ui.px(32)
            color: Qt.alpha("black", 0.45)
        }
        Rectangle {
            id: card
            anchors.fill: parent
            radius: ui.px(14)
            gradient: Gradient {
                GradientStop { position: 0; color: ui.tint(ui.deep, 0.12) }
                GradientStop { position: 1; color: ui.tint(ui.deep, 0.05) }
            }
            border { width: 1; color: Qt.alpha(ui.bright, 0.12) }
            Rectangle { // the gloss along its top
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 1 }
                height: ui.px(30)
                radius: parent.radius
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.alpha("white", 0.06) }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
            Beam { // the lit top edge
                anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: ui.px(12); rightMargin: ui.px(12) }
            }
        }

        // What the option pointed at does, in one line under all the rows:
        // the menu keeps its size while the pointer moves.
        Text {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: ui.px(20); bottomMargin: ui.px(12) }
            text: f.hint
            color: ui.dim
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
            font { family: ui.sans; pixelSize: ui.px(11) }
        }

        Column {
            id: list
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: ui.px(8) }
            Repeater {
                model: store.menuRows
                Item {
                    id: row
                    required property int index
                    required property var modelData
                    readonly property bool pickable: store.menuPickable(modelData)
                    readonly property bool option: modelData.option !== undefined
                    readonly property bool colors: modelData.option === "cover_colors"
                    readonly property bool on: { daemon.options; return option && store.optionOn(modelData.option) }
                    readonly property bool picked: pickable && (store.menuSel === index || rowHover.hovered)
                    width: list.width
                    height: modelData.sep ? ui.px(11) : modelData.heading ? ui.px(28) : ui.px(32)
                    onPickedChanged: if (picked && option) f.hint = modelData.hint || ""
                                     else if (!picked && f.hint === (modelData.hint || "")) f.hint = ""

                    // A divider: a hairline fading out at both ends.
                    Rectangle {
                        visible: !!row.modelData.sep
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: ui.px(10) }
                        height: 1
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: "transparent" }
                            GradientStop { position: 0.5; color: Qt.alpha(ui.bright, 0.16) }
                            GradientStop { position: 1; color: "transparent" }
                        }
                    }
                    // A heading, as the list's shelves have.
                    Text {
                        visible: !!row.modelData.heading
                        anchors { left: parent.left; leftMargin: ui.px(12); bottom: parent.bottom; bottomMargin: ui.px(6) }
                        text: (row.modelData.heading || "").toUpperCase()
                        color: ui.here
                        font { family: ui.sans; pixelSize: ui.px(10); weight: Font.Bold; letterSpacing: 1.6 }
                    }

                    RowLight {
                        anchors.fill: parent
                        visible: row.picked
                    }
                    Text {
                        id: glyph
                        visible: !!row.modelData.icon
                        anchors { left: parent.left; leftMargin: ui.px(12); verticalCenter: parent.verticalCenter }
                        width: ui.px(18)
                        text: row.modelData.icon || ""
                        color: row.picked ? ui.here : ui.dim
                        font { family: ui.mono; pixelSize: ui.px(15) }
                    }
                    Column {
                        id: words
                        visible: row.pickable
                        anchors { left: parent.left; leftMargin: row.option ? ui.px(12) : ui.px(42); right: tail.left; rightMargin: ui.px(10); verticalCenter: parent.verticalCenter }
                        spacing: ui.px(1)
                        Text {
                            width: parent.width
                            // The colors row names what it is set to: theme or cover.
                            text: row.colors ? (row.on ? "Cover Colors" : "Theme Colors") : (row.modelData.label || "")
                            color: row.picked ? ui.bright : ui.fg
                            elide: Text.ElideRight
                            font { family: ui.sans; pixelSize: ui.px(13); weight: row.picked ? Font.DemiBold : Font.Normal }
                        }
                    }
                    Item {
                        id: tail
                        anchors { right: parent.right; rightMargin: ui.px(12); verticalCenter: parent.verticalCenter }
                        width: row.colors ? pick.width : row.option ? toggle.width : key.implicitWidth
                        height: parent.height
                        Toggle {
                            id: toggle
                            visible: row.option && !row.colors
                            anchors.verticalCenter: parent.verticalCenter
                            on: row.on
                        }
                        // Theme or cover: the two halves of one pill, the
                        // chosen one lit; a click goes over to the other.
                        Rectangle {
                            id: pick
                            visible: row.colors
                            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                            width: halves.implicitWidth + ui.px(4)
                            height: ui.px(22)
                            radius: height / 2
                            color: Qt.alpha(ui.deep, 0.6)
                            border { width: 1; color: Qt.alpha(ui.bright, 0.16) }
                            Row {
                                id: halves
                                anchors.centerIn: parent
                                Repeater {
                                    model: ["Theme", "Cover"]
                                    Rectangle {
                                        required property int index
                                        required property string modelData
                                        readonly property bool lit: (index === 1) === row.on
                                        width: word.implicitWidth + ui.px(14)
                                        height: ui.px(18)
                                        radius: height / 2
                                        color: lit ? ui.here : "transparent"
                                        Behavior on color { enabled: !ui.calm; ColorAnimation { duration: 160 } }
                                        Text {
                                            id: word
                                            anchors.centerIn: parent
                                            text: parent.modelData
                                            color: parent.lit ? (theme.dark ? ui.deep : "white") : ui.dim
                                            font { family: ui.sans; pixelSize: ui.px(10); weight: Font.DemiBold }
                                        }
                                    }
                                }
                            }
                        }
                        Text { // its key, a note about the row
                            id: key
                            visible: row.modelData.act !== undefined
                            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                            text: row.modelData.act || ""
                            color: row.picked ? ui.fg : ui.dim
                            font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold }
                        }
                    }
                    HoverHandler { id: rowHover; enabled: row.pickable; cursorShape: Qt.PointingHandCursor }
                    TapHandler { enabled: row.pickable; onTapped: store.menuRow(row.index) }
                }
            }
        }
    }
}
