// Cathedral: a stained-glass window in a dark church, and the sun coming
// through it, after psp-cathedral. The glass throws its colors onto the
// marble floor and the piers, shafts of colored light stand in the haze,
// dust drifts through them and lights up only where the sun catches it.
// The camera drifts slowly through the nave while the sun crosses and
// clouds pass. A kick makes the sun swell, the bass thickens the haze, the
// highs make the dust glitter, each piece of glass glows with its part of
// the spectrum, and every 24th kick brings another church's window.
// Four passes: the window flat, with its mip levels for the blur
// (shaders/viz_cathedral_glass.frag), then the church in three parts of
// one shader (shaders/viz_cathedral.frag): the shafts in the air at
// a third of the size, fifteen times a second, the church at half, the glass over it at full size.
import QtQuick
import ".."

VizEffect {
    id: fx
    backgroundColor: "#08090d"
    adaptiveScene: true

    property real tick: 0
    property real pan: 0
    property real swell: 0
    property real glow: 0
    property int kicks: -1
    property int design: 0
    property int old: 0
    property real mixing: 1

    VizFrame {
        running: fx.running
        onTriggered: {
            const a = fx.audio, dt = Math.min(frameTime, 0.1)
            fx.tick += dt * (0.3 + 0.7 * a.live)
            fx.pan += dt // the camera keeps a steady pace, independent of the music
            if (a.kicks !== fx.kicks) {
                if (fx.kicks >= 0) fx.swell = Math.min(fx.swell + 0.35, 0.7)
                fx.kicks = a.kicks
            }
            fx.swell *= Math.exp(-dt * 2.5)
            fx.glow += (a.live * (0.5 + 0.8 * a.level) - fx.glow) * Math.min(1, dt * 2)
            const d = Math.floor(a.kicks / 24) % 3
            if (d !== fx.design) { fx.old = fx.design; fx.design = d; fx.mixing = 0 }
            if (fx.mixing < 1) fx.mixing = Math.min(1, fx.mixing + dt / 4)
        }
    }

    // The window, flat, into its texture only.
    ShaderEffect {
        id: pane
        width: 768
        height: 1190
        visible: true
        blending: false
        property real tick: fx.tick
        property real design: fx.design
        property real old: fx.old
        property real mixing: fx.mixing
        property real glow: fx.glow
        property var spectrum: fx.spectrum
        fragmentShader: "qrc:/shaders/viz_cathedral_glass.frag.qsb"
    }
    // The glass glows slowly with the music: fifteen times a second is
    // plenty, and saves drawing its pieces every frame.
    Timer {
        interval: 66
        repeat: true
        running: fx.running
        onTriggered: { window.scheduleUpdate(); air.scheduleUpdate() }
    }
    ShaderEffectSource {
        id: window
        sourceItem: pane
        hideSource: true
        live: false
        mipmap: true
        smooth: true
        textureSize: Qt.size(768, 1190)
    }

    // The church: one shader, its part chosen. The shafts are soft light,
    // gathered through the air at a third of the size into a texture of
    // their own; the church around them, at half, adds them; the glass,
    // the only sharp thing, comes last at full size.
    component Church: ShaderEffect {
        property vector4d activeArea: fx.activeArea
        property real tick: fx.tick
        property real pan: fx.pan
        property real aspect: width / Math.max(1, height)
        // the sun crosses over some minutes; clouds dim it now and then
        property real sunx: 0.38 + 0.3 * Math.sin(fx.tick * 0.021)
        property real sun: (0.62 + 0.38 * Math.max(0, Math.min(1, 0.6 + 0.5 * Math.sin(fx.tick * 0.13) * Math.sin(fx.tick * 0.071 + 1.3)))) * (1 + fx.swell)
        property real haze: fx.audio.bass
        property real sparkle: fx.audio.high
        property real beat: fx.audio.beat
        property color tint: fx.here
        property var glass: window
        fragmentShader: "qrc:/shaders/viz_cathedral.frag.qsb"
    }
    Church {
        id: shafts
        width: Math.max(2, Math.round(fx.width / 3))
        height: Math.max(2, Math.round(fx.height / 3))
        blending: false
        property real part: 1
        property var air: window // unread in this pass
    }
    // The shafts change only as slowly as the camera drifts and the sun
    // moves: gathered fifteen times a second, with the glass, they can
    // afford many fine steps.
    ShaderEffectSource {
        id: air
        sourceItem: shafts
        hideSource: true
        live: false
        smooth: true
        format: ShaderEffectSource.RGBA16F
    }
    Church {
        anchors.fill: parent
        property real part: 0
        property var air: air
        // soft light in the dark: half the size, drawn up smoothly
        layer.enabled: true
        layer.smooth: true
        layer.textureSize: Qt.size(Math.max(2, Math.round(width / 2)), Math.max(2, Math.round(height / 2)))
    }
    // and the glass over it at full size, sharp: little to draw, the rest
    // of the screen left alone
    Church {
        anchors.fill: parent
        property real part: 2
        property var air: air
    }
}
