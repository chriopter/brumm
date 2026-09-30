// Led: the hi-fi graphic equalizer, as a 2000s stereo's display would
// show it — LED columns behind smoked glass in a chrome rim, peak holds
// floating, glowing onto a glossy floor. All of it one shader.
import QtQuick

VizEffect {
    id: fx
    ShaderEffect {
        anchors.fill: parent
        property real aspect: width / Math.max(1, height)
        // as many columns as fit comfortably, up to 32; segments to match
        property real cols: Math.max(8, Math.min(32, Math.round(width / ui.px(44))))
        property real rows: Math.max(10, Math.min(26, Math.round(height / ui.px(28))))
        property real beat: fx.audio.beat
        property color tint: fx.here
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_led.frag.qsb"
    }
}
