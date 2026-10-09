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
    // Small stages can shrink the cover to zero. Keep the grid bounded
    // while it is hidden or resized instead of dividing by that size.
    readonly property real tile: Math.max(ui.px(40), size / 3)
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
    readonly property color ground: ui.tint(ui.deep, 0.14) // what the dimmed covers sink into
    // Only where there is room above and below the cover does the wall go
    // up; a short window shows the cover alone.
    // (or beside it, in a wide one)
    readonly property bool roomy: height >= size + tile * 0.8 || width >= size + tile * 3

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

    // Which cover each tile shows.
    property var assigned: []
    // Counts the times the wall was dealt anew: each sends a wave through
    // every tile, from the top right down to the bottom left, as through
    // water; the tiles rise, turn over and settle with what they now show.
    property int wave: 0
    // One deal, one wave, per song: the news of a new song comes in parts
    // over a second or two (the song, what comes next, what played), and
    // only the first settled state is dealt. Later news waits for the next
    // song, unless tiles are still empty.
    property string dealtFor: "-"
    onPoolChanged: {
        const filled = assigned.filter(a => !!a).length
        if (url !== dealtFor || (filled < cells.length && pool.length > filled)) settle.restart()
    }
    onCellsChanged: { dealtFor = "-"; settle.restart() }
    Timer {
        id: settle
        interval: 700
        onTriggered: {
            const next = wall.cells.map((c, i) => wall.pool[i] || null)
            const same = next.length === wall.assigned.length && next.every((n, i) => (n ? n.art : "") === (wall.assigned[i] ? wall.assigned[i].art : ""))
            wall.dealtFor = wall.url
            wall.assigned = next
            if (!same) wall.wave++
        }
    }

    // The tiles, ordered by their distance to the cover, so the nearest
    // get what comes next.
    readonly property var cells: {
        const out = []
        if (!visible || !roomy) return out
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
            property vector2d fadeSpan: Qt.vector2d(wall.tile * 1.2, Math.max(ui.px(30), Math.min(ui.px(400), covers.height / 2 - wall.size / 2)))
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
                // Without a wave: the same cover with new news, or the first one.
                onItChanged: if (!showing || ui.calm || (it && showing.art === it.art)) showing = it
                Connections {
                    target: wall
                    function onWaveChanged() {
                        if (ui.calm || !(t.showing || t.it)) { t.showing = t.it; return }
                        swap.restart()
                    }
                }
                // The tile turns over, the next cover waiting on its back, when
                // the wave reaches it.
                property real turn: 0
                property real lift: 0 // it rises off the wall while it turns
                SequentialAnimation {
                    id: swap
                    PauseAnimation { duration: 150 + (wall.cols - 1 - t.modelData.c + t.modelData.r) * 55 }
                    ParallelAnimation {
                        NumberAnimation { target: t; property: "turn"; to: 90; duration: 220; easing.type: Easing.InQuad }
                        NumberAnimation { target: t; property: "lift"; to: 0.18; duration: 220; easing.type: Easing.OutQuad }
                    }
                    ScriptAction { script: { t.showing = t.it; t.turn = -90 } }
                    ParallelAnimation {
                        NumberAnimation { target: t; property: "turn"; to: 0; duration: 300; easing.type: Easing.OutCubic }
                        NumberAnimation { target: t; property: "lift"; to: 0; duration: 300; easing.type: Easing.InQuad }
                    }
                }
                transform: Rotation {
                    origin.x: t.width / 2
                    origin.y: t.height / 2
                    axis { x: 0; y: 1; z: 0 }
                    angle: t.turn
                }
                readonly property bool hot: hover.hovered
                x: wall.x0 + modelData.c * wall.tile + wall.gap / 2
                y: wall.y0 + modelData.r * wall.tile + wall.gap / 2
                width: wall.tile - wall.gap
                height: width
                z: hot || lift > 0 ? 3 : 1
                // Further out, fainter: the wall sinks into the light.
                readonly property real rest: Math.max(0.18, 0.62 - modelData.d * 0.07)
                scale: (hot ? 1.16 : 1) * (1 + lift)
                Behavior on scale { enabled: !ui.calm; NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }

                RowLight { anchors.fill: parent; center: 0.5; strength: t.hot ? 1 : 0; Behavior on strength { enabled: !ui.calm; NumberAnimation { duration: 260 } } }
                Rectangle { // an empty tile: glass, until a cover comes
                    anchors.fill: parent
                    visible: !t.showing
                    radius: ui.coverRadius
                    color: Qt.alpha(ui.bright, 0.04)
                }
                Rectangle { // its ground: the backdrop's light passes behind the covers, not through
                    anchors.fill: parent
                    visible: !!t.showing
                    radius: ui.coverRadius
                    color: wall.ground
                }
                Image { // what it shows next, loaded before it turns: the turn never waits for a picture
                    visible: false
                    source: t.it ? "image://cover/" + encodeURIComponent(t.it.art) : ""
                    sourceSize: art.sourceSize
                    asynchronous: true
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
                    visible: ui.effects && (art.status === Image.Ready || art.status === Image.Loading)
                    property var source: art
                    property vector2d size: Qt.vector2d(width, height)
                    property real radius: ui.coverRadius
                    fragmentShader: "qrc:/shaders/tile.frag.qsb"
                }
                }
                Rectangle { // further out, darker: the cover sinks into its ground
                    anchors.fill: parent
                    visible: !!t.showing
                    radius: ui.coverRadius
                    color: wall.ground
                    opacity: t.hot ? 0 : 1 - t.rest
                    Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 260; easing.type: Easing.InOutQuad } }
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
                        wall.fly(t) // it becomes the cover playing
                        if (w.next !== undefined) store.jumpTo(w.next)
                        else if (w.played) store.playSong(w.played)
                        else if (w.album) store.playItem(w.album)
                    }
                }
            }
        }
    }

    // A cover picked from the wall flies up to where the playing one is,
    // growing, and lands as it.
    function fly(tile) {
        if (ui.calm) return
        land.stop()
        flyArt = tile.showing.art
        flyer.opacity = 1
        flyer.x = tile.x; flyer.y = tile.y; flyer.width = tile.width; flyer.height = tile.height
        flight.restart()
        giveUp.restart()
    }
    // It stays on the cover's place until the cover itself shows it: the
    // player takes a moment to start an album, and the old cover must not
    // show through in between.
    property string flyArt: ""
    onUrlChanged: if (flyArt !== "" && url === flyArt && !flight.running) land.restart()
    Timer { id: giveUp; interval: 6000; onTriggered: if (wall.flyArt !== "") land.restart() }
    Item {
        id: flyer
        z: 4
        visible: wall.flyArt !== ""
        // The picture the tile already has, so it starts at once; with the
        // cover's rounded corners all the way.
        Image {
            id: flyImg
            anchors.fill: parent
            visible: !ui.effects
            source: wall.flyArt ? "image://cover/" + encodeURIComponent(wall.flyArt) : ""
            sourceSize: Qt.size(Math.round(wall.restTile * 1.3), Math.round(wall.restTile * 1.3))
            fillMode: Image.PreserveAspectCrop
            smooth: true
        }
        ShaderEffect {
            anchors.fill: parent
            visible: ui.effects
            property var source: flyImg
            property vector2d size: Qt.vector2d(width, height)
            property real radius: ui.coverRadius
            fragmentShader: "qrc:/shaders/tile.frag.qsb"
        }
        ParallelAnimation {
            id: flight
            NumberAnimation { target: flyer; property: "x"; to: card.x; duration: 420; easing.type: Easing.OutCubic }
            NumberAnimation { target: flyer; property: "y"; to: card.y; duration: 420; easing.type: Easing.OutCubic }
            NumberAnimation { target: flyer; property: "width"; to: card.width; duration: 420; easing.type: Easing.OutCubic }
            NumberAnimation { target: flyer; property: "height"; to: card.height; duration: 420; easing.type: Easing.OutCubic }
            onFinished: if (wall.url === wall.flyArt) land.restart()
        }
        SequentialAnimation {
            id: land
            PauseAnimation { duration: 400 } // the cover fades its new art in under it
            NumberAnimation { target: flyer; property: "opacity"; to: 0; duration: 200 }
            ScriptAction { script: wall.flyArt = "" }
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
