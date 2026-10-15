import QtQuick
import ".."

VizEffect {
    id: fx
    backgroundColor: ui.deep
    adaptiveScene: true
    RenderSurface { id: surface; anchors.fill: parent; density: 0.5 }
    Backdrop {
        anchors.fill: parent
        running: fx.running
        layer.enabled: ui.effects
        layer.smooth: true
        layer.textureSize: surface.textureSize
    }
}
