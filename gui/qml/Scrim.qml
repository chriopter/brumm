// Scrim dims the window under a popup; a click on it closes the popup.
import QtQuick

Rectangle {
    anchors.fill: parent
    color: Qt.alpha(ui.deep, 0.35)
    TapHandler {
        onTapped: {
            if (store.upd && (store.upd.checking || store.upd.installing)) return
            if (store.barAsk) return store.barAnswer(false)
            store.help = false
            store.pick = null
            if (store.upd) store.updateKey("esc")
        }
    }
}
