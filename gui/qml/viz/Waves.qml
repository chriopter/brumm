import QtQuick
import ".."

// Waves is the window's backdrop, drawn at full size: a layer at half
// the pixels drew a second frame after every tick and cost more than it
// saved. On large screens the stage draws it smaller anyway (FullStage's
// shrink).
VizEffect {
    id: fx
    backgroundColor: ui.deep
    adaptiveScene: true
    Backdrop {
        anchors.fill: parent
        running: fx.running
    }
}
