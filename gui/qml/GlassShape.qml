// GlassShape is the glass of a button (shaders/glass.frag), round when
// its corners are half its height.
import QtQuick

ShaderEffect {
    property vector2d size: Qt.vector2d(width, height)
    property real radius: height / 2
    property real lit: 0
    property real hot: 0
    property color tint: ui.here
    Behavior on hot { enabled: !ui.calm; NumberAnimation { duration: 140 } }
    fragmentShader: "qrc:/shaders/glass.frag.qsb"
}
