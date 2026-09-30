// BarAskBox asks once, on the first start, about the bar widget.
import QtQuick

Box {
    title: "top bar"
    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: "Show brumm in the Omarchy bar?"; color: ui.bright; font { family: ui.sans; pixelSize: ui.px(16); weight: Font.DemiBold } }
    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: "The options can change it later."; color: ui.dim; font { family: ui.sans; pixelSize: ui.px(12) } }
    Item { width: 1; height: ui.px(8) }
    KeyHints { pairs: [["enter", "yes"], ["esc", "no"]] }
    TapHandler { onTapped: store.barAnswer(true) }
}
