import QtQuick
import ".."

ShaderEffectSource {
    id: buffer
    property bool running: false
    live: false
    VizFrame {
        running: buffer.running
        onTriggered: buffer.scheduleUpdate()
    }
}
