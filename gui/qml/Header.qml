// Header runs across the top: where you are, the stack's last title large
// and the way there small, and under it a glowing line.
import QtQuick

Item {
    id: h
    height: ui.px(52)

    Text {
        id: crumbs
        anchors { left: parent.left; leftMargin: ui.px(2); verticalCenter: parent.verticalCenter; verticalCenterOffset: -ui.px(4) }
        width: Math.min(implicitWidth, h.width - (filter.visible ? filter.width + ui.px(18) : 0) - ui.px(4))
        textFormat: Text.StyledText
        elide: Text.ElideLeft
        color: ui.bright
        style: ui.lift
        styleColor: ui.liftColor
        font { family: ui.sans; pixelSize: ui.px(22); weight: Font.DemiBold }
        text: {
            const esc = t => t.replace(/&/g, "&amp;").replace(/</g, "&lt;")
            const parts = store.crumbs.split("  /  ").map(esc)
            const last = parts.pop()
            return (parts.length ? "<font color='" + Qt.alpha(ui.bright, 0.5) + "'>" + parts.join("  ›  ") + "  ›  </font>" : "") + last
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
