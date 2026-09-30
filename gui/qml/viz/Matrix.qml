// matrix: digital rain. Glyphs (half-width katakana, digits and marks)
// fall in columns, each head white-hot, its trail fading from bright green
// into the dark as the rain moves on. The spectrum decides where it rains
// (bass in the middle), the highs how hard and how fast the glyphs
// mutate, the loudness how fast it falls; a kick sends a squall down from
// the top and flashes the trails. Behind it a second, smaller rain.
import QtQuick

VizEffect {
    id: fx

    // The grid: a cell a glyph.
    readonly property int cellW: Math.max(8, Math.round(ui.px(15)))
    readonly property int cellH: Math.max(10, Math.round(ui.px(21)))
    readonly property int cols: Math.max(1, Math.ceil(width / cellW))
    readonly property int rows: Math.max(1, Math.ceil(height / cellH))

    // The rain, moved on once a frame: how fast it falls (rows a second),
    // how far it has fallen, how far the glyphs have churned, and where it
    // stood at the last two kicks.
    property real flow: 0
    property real fall: 0
    property real fallPrev: 0
    property real decay: 1
    property real churn: 0
    property real flash: 0
    property real high: 0
    property real level: 0
    property real time: 0
    property real kick0: -1000
    property real kick1: -1000
    property int kicks: 0
    property int seen: -1
    property real last: -1

    Connections {
        target: fx.running ? fx.audio : null
        function onTicked() {
            const a = fx.audio
            const dt = fx.last < 0 ? 0 : Math.min(0.1, Math.max(0, a.t - fx.last))
            fx.last = a.t
            const target = fx.rows * (0.3 + a.live * (0.2 + 1.2 * a.level))
            fx.flow += (target - fx.flow) * Math.min(1, dt * 2)
            fx.fallPrev = fx.fall
            fx.fall += fx.flow * dt
            // trails a third of the screen (at least 8 rows) long at any speed
            fx.decay = Math.exp(-2.8 * fx.flow * dt / (fx.rows * 0.35 + 8))
            fx.churn += dt * (0.25 + 5 * a.high)
            fx.high = a.high
            fx.level = a.level
            fx.time += dt
            if (a.kicks !== fx.seen) {
                if (fx.seen >= 0) {
                    fx.kick1 = fx.kick0
                    fx.kick0 = fx.fall
                    fx.kicks++
                    fx.flash = 1
                }
                fx.seen = a.kicks
            }
            fx.flash *= Math.exp(-dt * 5)
            state.scheduleUpdate()
        }
    }
    onRunningChanged: last = -1

    // The glyphs, drawn once into a strip the shader reads them from.
    readonly property string glyphs: "ｦｱｳｴｵｶｷｹｺｻｼｽｾｿﾀﾂﾃﾅﾆﾇﾈﾊﾋﾎﾏﾐﾑﾒﾓﾔﾕﾗﾘﾜ2598Z*):.\"=+-¦|_"
    Row {
        id: strip
        Repeater {
            model: fx.glyphs.length
            Item {
                required property int index
                width: 48
                height: 64
                Text {
                    anchors.centerIn: parent
                    text: fx.glyphs[parent.index]
                    color: "white"
                    font { family: ui.mono; pixelSize: 44; weight: Font.DemiBold }
                }
            }
        }
    }
    ShaderEffectSource {
        id: atlas
        sourceItem: strip
        hideSource: true
        live: false
        mipmap: true
        smooth: true
    }

    // The rain's state, a texel a cell: how lit each cell is, which glyph
    // it shows, whether a head is on it. Each frame works out the next
    // from the last.
    ShaderEffect {
        id: stepper
        width: fx.width
        height: fx.height
        property var prev: state
        property var spectrum: fx.spectrum
        property size grid: Qt.size(fx.cols, fx.rows)
        property real fall: fx.fall
        property real fallPrev: fx.fallPrev
        property real decay: fx.decay
        property real high: fx.high
        property real kick0: fx.kick0
        property real kick1: fx.kick1
        property real kicks: fx.kicks
        blending: false
        fragmentShader: "qrc:/shaders/viz_matrix_step.frag.qsb"
    }
    ShaderEffectSource {
        id: state
        sourceItem: stepper
        hideSource: true
        live: false
        recursive: true
        smooth: true
        mipmap: true
        // a row more, below the screen: which drops fall
        textureSize: Qt.size(fx.cols, fx.rows + 1)
    }

    // The rain as it shows.
    ShaderEffect {
        anchors.fill: parent
        property var state: state
        property var atlas: atlas
        property size grid: Qt.size(fx.cols, fx.rows)
        property real glyphCount: fx.glyphs.length
        property real churn: fx.churn
        property real lod: Math.log2(48 / fx.cellW)
        property real flash: fx.flash
        property real time: fx.time
        property real level: fx.level
        property real aspect: width / Math.max(1, height)
        property color tint: fx.here
        fragmentShader: "qrc:/shaders/viz_matrix.frag.qsb"
    }
}
