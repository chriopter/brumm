// VizEffect is one visualizer style: the window's visualizer (FullStage)
// loads one per style from qml/viz/<Name>.qml, each named and built as the
// terminal player's style of that name (internal/tui/viz_*.go), drawn as
// the window draws: shaders, light, color.
//
// What a style has to work with:
//   running   — it shows: animate. When false, stand still (it is fading
//               out, or the window is minimized).
//   audio     — the shared listener (src/vizaudio.h): t (seconds), live
//               (1 playing … 0 paused), bass, mid, high, level (0–1,
//               smoothed), beat (1 on a kick, decaying), kicks (count),
//               since (seconds since the last kick), sm (64 smoothed
//               bands as numbers, for JS; shaders use the textures).
//   spectrum  — a texture, 64 × 4, values in red, 0–1: row 0 the bands,
//               row 1 smoothed, row 2 falling peaks, row 3 the waveform
//               (0.5 silence). Sample at y = (row + 0.5) / 4.
//   wave      — a texture, 256 × 1: the waveform, 0.5 silence.
//   here, accent, colors — the cover's color, the theme's accent, and the
//               theme's colors by name (colors.red, colors.background…).
//
// Everything per pixel belongs in a shader (gui/shaders/viz_<name>.frag,
// built in as "qrc:/shaders/viz_<name>.frag.qsb"); JavaScript runs at most
// once a frame and only for small things. A style may keep state between
// frames with a feedback ShaderEffectSource (recursive: true).
import QtQuick

Item {
    id: fx
    property bool running: false
    readonly property var audio: vizAudio
    property var spectrum: null
    property var wave: null
    readonly property color here: ui.here
    readonly property color accent: ui.accent
    readonly property var colors: theme.colors
}
