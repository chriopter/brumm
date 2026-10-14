// Keep offscreen render targets allocated across live layout changes.
// Geometry still follows every resize; only storage is rounded and reused.
import QtQuick
import QtQuick.Window

Item {
    id: surface
    visible: false
    property real density: Screen.devicePixelRatio
    readonly property size wanted: Qt.size(
        Math.max(128, Math.ceil(width * density / 128) * 128),
        Math.max(128, Math.ceil(height * density / 128) * 128))
    property size textureSize: Qt.size(128, 128)
    onWantedChanged: {
        textureSize = Qt.size(Math.max(textureSize.width, wanted.width),
                              Math.max(textureSize.height, wanted.height))
        release.restart()
    }
    Timer {
        id: release
        interval: 180
        onTriggered: surface.textureSize = surface.wanted
    }
}
