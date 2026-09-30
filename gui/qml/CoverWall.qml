// CoverWall is the stage's other way to show what plays: a wall of covers,
// all the same size, running on past the stage's edges and fading into the
// light, with the cover playing set into its middle three tiles wide.
// What comes next sits nearest to it, then what played, then the
// library's albums, over and over. Pointed at, a tile comes forward the
// way iTunes' covers did: it eases out toward you, catches a gloss and
// lights up behind. Clicked, it plays (what comes next, what played) or
// opens (an album). Static otherwise: drawn again only when it changes.
import QtQuick

Item {
    id: wall
    property string url
    property real size: ui.px(360)           // the cover playing, three tiles wide
    readonly property real tile: size / 3
    readonly property int gap: ui.px(5)
    // Enough tiles to run past every edge; odd counts keep the cover centered.
    // How many tiles: counted from the size once it rests, so dragging the
    // divider only slides the wall along instead of building it anew each
    // step (it is centred on the live size meanwhile).
    property real restW: width
    property real restH: height
    onWidthChanged: resting.restart()
    onHeightChanged: resting.restart()
    onSizeChanged: resting.restart()
    property real restTile: tile // the size the covers are loaded at: again only once it rests
    Timer { id: resting; interval: 180; onTriggered: { wall.restW = wall.width; wall.restH = wall.height; wall.restTile = wall.tile } }
    readonly property int cols: Math.ceil(restW / tile / 2) * 2 + 1
    readonly property int rows: Math.ceil(restH / tile / 2) * 2 + 1
    readonly property int mc: (cols - 3) / 2 // the cover's first column and row
    readonly property int mr: (rows - 3) / 2
    readonly property real x0: (width - cols * tile) / 2
    readonly property real y0: (height - rows * tile) / 2
    clip: true
    // Only where there is room above and below the cover does the wall go
    // up; a short window shows the cover alone.
    readonly property bool roomy: height >= size + tile * 0.8

    // Every cover to put up, each once, the likeliest first: what comes
    // next, what played, then the albums and songs the lists know.
    readonly property var pool: {
        store.rev
        const out = [], seen = new Set([url])
        const add = (art, what) => { if (art && !seen.has(art)) { seen.add(art); out.push({ art: art, what: what }) } }
        store.upNext.forEach((t, i) => add(t.artwork, { next: i }))
        store.recent.slice().reverse().forEach(t => add(t.artwork, { played: t }))
        for (const stack of store.stacks)
            for (const v of stack)
                for (const r of v.rows) {
                    if (out.length >= cells.length) return out
                    if (r.item && r.item.kind === "album") add(r.item.artwork, { album: r.item })
                    else if (r.track) add(r.track.artwork, { played: r.track })
                }
        return out
    }
    // The albums, to have covers to put up from the start.
    Component.onCompleted: {
        const albums = store.stacks[store.secAlbums]
        if (albums && !albums[0].loaded) store.load(albums[0])
    }

    // Which cover each tile shows. A tile keeps its cover while that is
    // still to be shown; only the tiles whose cover went take new ones,
    // each fading over by itself: nothing moves around.
    property var assigned: []
    onPoolChanged: settle.restart()
    onCellsChanged: settle.restart()
    Timer {
        id: settle
        interval: 300 // a new song's news comes in parts: take them together
        onTriggered: {
            const byArt = new Map(wall.pool.map(p => [p.art, p]))
            const next = new Array(wall.cells.length).fill(null)
            const used = new Set()
            wall.assigned.forEach((a, i) => {
                if (i < next.length && a && byArt.has(a.art)) { next[i] = byArt.get(a.art); used.add(a.art) }
            })
            const fresh = wall.pool.filter(p => !used.has(p.art))
            for (let i = 0; i < next.length && fresh.length; i++)
                if (!next[i]) next[i] = fresh.shift()
            wall.assigned = next
        }
    }

    // The tiles, ordered by their distance to the cover, so the nearest
    // get what comes next.
    readonly property var cells: {
        const out = []
        for (let r = 0; r < rows; r++)
            for (let c = 0; c < cols; c++)
                if (c < mc || c > mc + 2 || r < mr || r > mr + 2)
                    out.push({ c: c, r: r, d: Math.hypot(c - mc - 1, (r - mr - 1) * 1.2) })
        return out.sort((a, b) => a.d - b.d)
    }

    // The wall, fading out toward all its edges.
    Item {
        id: covers
        anchors.fill: parent
        layer.enabled: ui.effects
        layer.effect: ShaderEffect {
            property vector2d wallSize: Qt.vector2d(covers.width, covers.height)
            // Up and down: 500 px of wall beyond the cover, the outer 400
            // fading; across: to the stage's edges.
            property vector2d fadeEnd: Qt.vector2d(covers.width / 2, Math.min(covers.height / 2, wall.size / 2 + ui.px(500)))
            property vector2d fadeSpan: Qt.vector2d(wall.tile * 1.2, ui.px(400))
            fragmentShader: "qrc:/shaders/vignette.frag.qsb"
        }

        Repeater {
            model: wall.roomy ? wall.cells : []
            Item {
                id: t
                required property int index
                required property var modelData
                readonly property var it: wall.assigned[index] || null
                // The cover on show, taken over from it with a fade.
                property var showing: null
                onItChanged: {
                    if (!showing || !it || ui.calm || showing.art === it.art) { showing = it; return }
                    swap.restart()
                }
                SequentialAnimation {
                    id: swap
                    NumberAnimation { target: face; property: "opacity"; to: 0; duration: 220; easing.type: Easing.InQuad }
                    ScriptAction { script: t.showing = t.it }
                    NumberAnimation { target: face; property: "opacity"; to: 1; duration: 420; easing.type: Easing.OutCubic }
                }
                readonly property bool hot: hover.hovered
                x: wall.x0 + modelData.c * wall.tile + wall.gap / 2
                y: wall.y0 + modelData.r * wall.tile + wall.gap / 2
                width: wall.tile - wall.gap
                height: width
                z: hot ? 3 : 1
                // Further out, fainter: the wall sinks into the light.
                readonly property real rest: Math.max(0.18, 0.62 - modelData.d * 0.07)
                opacity: hot ? 1 : rest
                scale: hot ? 1.16 : 1
                Behavior on scale { enabled: !ui.calm; NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }
                Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 260; easing.type: Easing.InOutQuad } }

                RowLight { anchors.fill: parent; center: 0.5; strength: t.hot ? 1 : 0; Behavior on strength { enabled: !ui.calm; NumberAnimation { duration: 260 } } }
                Rectangle { // an empty tile: glass, until a cover comes
                    anchors.fill: parent
                    visible: !t.showing
                    radius: ui.coverRadius
                    color: Qt.alpha(ui.bright, 0.04)
                }
                Item {
                    id: face
                    anchors.fill: parent
                Image {
                    id: art
                    anchors.fill: parent
                    visible: !ui.effects // with effects, drawn rounded below
                    source: t.showing ? "image://cover/" + encodeURIComponent(t.showing.art) : ""
                    sourceSize: Qt.size(Math.round(wall.restTile * 1.3), Math.round(wall.restTile * 1.3))
                    asynchronous: true
                    fillMode: Image.PreserveAspectCrop
                }
                ShaderEffect { // the art with the cover's rounded corners
                    anchors.fill: parent
                    visible: ui.effects && art.status === Image.Ready
                    property var source: art
                    property vector2d size: Qt.vector2d(width, height)
                    property real radius: ui.coverRadius
                    fragmentShader: "qrc:/shaders/tile.frag.qsb"
                }
                }
                Rectangle { // the gloss it catches, coming forward
                    anchors.fill: parent
                    opacity: t.hot ? 1 : 0
                    Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 260 } }
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.alpha("white", 0.28) }
                        GradientStop { position: 0.45; color: Qt.alpha("white", 0.04) }
                        GradientStop { position: 0.46; color: "transparent" }
                    }
                    border { width: 1; color: Qt.alpha(ui.bright, 0.45) }
                    radius: ui.coverRadius
                }
                HoverHandler { id: hover; enabled: !!t.showing; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    enabled: !!t.showing
                    onTapped: {
                        const w = t.showing.what
                        if (w.next !== undefined) store.jumpTo(w.next)
                        else if (w.played) store.playSong(w.played)
                        else if (w.album) store.openItem(w.album, "")
                    }
                }
            }
        }
    }

    // The cover playing, set into the wall; it turns over on a click.
    RowLight { x: card.x; y: card.y; width: card.width; height: card.height; center: 0.5; strength: 0.9 }
    CoverCard {
        id: card
        x: wall.x0 + wall.mc * wall.tile + wall.gap / 2
        y: wall.y0 + wall.mr * wall.tile + wall.gap / 2
        width: wall.tile * 3 - wall.gap
        height: width
        url: wall.url
    }
}
