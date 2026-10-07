// Header runs across the top: where you are, the stack's last title large
// and the way there small, and under it a glowing line.
import QtQuick

Item {
    id: h
    height: ui.px(52)

    Item {
        id: crumbs
        anchors { left: parent.left; leftMargin: ui.px(2); verticalCenter: parent.verticalCenter; verticalCenterOffset: -ui.px(4) }
        width: Math.min(trail.width, h.width - (filter.visible ? filter.width + ui.px(18) : 0) - ui.px(4))
        height: trail.height
        clip: true

        Row {
            id: trail
            x: Math.min(0, crumbs.width - width)
            Repeater {
                id: parts
                model: { store.rev; return store.stack().map(v => v.title) }
                Row {
                    id: part
                    required property int index
                    required property string modelData
                    readonly property bool ancestor: index < parts.count - 1
                    Text {
                        text: part.modelData
                        color: part.ancestor ? Qt.alpha(ui.bright, 0.5) : ui.bright
                        style: ui.lift
                        styleColor: ui.liftColor
                        font { family: ui.sans; pixelSize: ui.px(22); weight: Font.DemiBold; underline: click.containsMouse }
                        MouseArea {
                            id: click
                            anchors.fill: parent
                            enabled: part.ancestor
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: store.jumpToCrumb(part.index)
                        }
                    }
                    Text {
                        visible: part.ancestor
                        text: "  ›  "
                        color: Qt.alpha(ui.bright, 0.5)
                        style: ui.lift
                        styleColor: ui.liftColor
                        font { family: ui.sans; pixelSize: ui.px(22); weight: Font.DemiBold }
                    }
                }
            }
        }
    }

    // Filter this list, beside its name: the same as /.
    Orb {
        id: filter
        anchors { left: crumbs.right; leftMargin: ui.px(14); verticalCenter: crumbs.verticalCenter }
        visible: { store.rev; return store.filterable() && !store.filtering && !(store.view && store.view.all) }
        size: ui.px(30)
        icon: "󰈲"
        onClicked: store.startFilter()
    }

    // The glowing line: one even beam, fading out at both ends.
    Beam { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } }
}
