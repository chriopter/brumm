import QtQuick
import QtTest
import "../qml" as Brumm

TestCase {
    name: "Resize"
    when: windowShown
    visible: true
    width: 1200
    height: 800

    QtObject {
        id: ui
        readonly property bool effects: false
        readonly property bool calm: true
        readonly property color deep: "#101010"
        readonly property color bright: "#ffffff"
        readonly property color fg: "#dddddd"
        readonly property color dim: "#888888"
        readonly property color here: "#8080ff"
        readonly property color edge: "#333333"
        readonly property color glassHi: "#222222"
        readonly property color shade: "#000000"
        readonly property int coverRadius: 6
        readonly property string sans: "sans-serif"
        readonly property string mono: "monospace"
        function px(n) { return n }
        function tint(base, amount) { return base }
    }
    QtObject {
        id: theme
        readonly property bool dark: true
    }
    QtObject {
        id: store
        readonly property var st: ({})
        readonly property var upNext: []
        readonly property var recent: []
        readonly property var stacks: []
        readonly property int rev: 0
        readonly property int secAlbums: 0
        property var rows: []
        readonly property int sel: 0
        readonly property var view: ({ key: "albums:" })
        signal artworkChanged()
        signal selMoved()
    }
    Component {
        id: gridComponent
        Brumm.DiscoverGrid { width: 600; height: 500; visible: false }
    }
    Item { id: browser }
    Item { id: playbackStage; property real playbackTop: 0 }

    function test_hiddenDiscoveryDoesNotBuildLibraryCards() {
        const grid = createTemporaryObject(gridComponent, this)
        verify(grid !== null)
        const rows = []
        for (let i = 0; i < 200; ++i)
            rows.push({ item: { name: "Album " + i, kind: "album" } })
        store.rows = rows
        const cards = findChild(grid, "discoveryCards")
        compare(cards.count, 0)
        grid.width = 450
        compare(cards.count, 0)
        grid.visible = true
        compare(cards.count, rows.length)
        grid.visible = false
        compare(cards.count, 0)
        store.rows = []
    }

    Component {
        id: wallComponent
        Brumm.CoverWall { width: 1000; height: 700; size: 360; url: "" }
    }

    function test_wallKeepsTilesDuringContinuousResize() {
        const wall = createTemporaryObject(wallComponent, this)
        verify(wall !== null)
        wait(220)
        const cells = wall.cells
        verify(cells.length > 0)
        // Cross several row/column thresholds, and the roomy threshold.
        // Rebuilding the model here destroys every tile during a drag.
        for (let i = 0; i < 8; ++i) {
            wall.width = 1000 - i * 60
            wall.height = 700 - i * 40
            wall.size = 360 + i * 15
            wait(16)
            verify(wall.cells === cells, "Tile delegates must survive the drag")
        }
        tryCompare(wall, "restW", wall.width, 500)
        tryCompare(wall, "restH", wall.height, 500)
        verify(wall.cells !== cells, "The grid must adapt after the drag")
        compare(wall.roomy, false)
    }

    function test_wallGrowthKeepsExistingDelegates() {
        const wall = createTemporaryObject(wallComponent, this)
        wait(220)
        const tile = findChild(wall, "wallTile:2:0")
        verify(tile !== null)
        const model = findChild(wall, "wallTiles")
        const before = model.count
        wall.width = 2200
        wall.height = 1400
        verify(model.count > before, "New slots must appear during the drag")
        compare(findChild(wall, "wallTile:2:0"), tile)
        wait(250)
        compare(findChild(wall, "wallTile:2:0"), tile, "Settling must preserve existing images")
    }

    function test_fallbackRepeatsAlreadyLoadedArtwork() {
        const wall = createTemporaryObject(wallComponent, this)
        wall.visible = false
        wall.readyArt = { a: { art: "a" }, b: { art: "b" } }
        for (let x = -6; x <= 6; ++x) {
            const cover = wall.fallback(x + ":3")
            verify(cover.art === "a" || cover.art === "b")
        }
        verify(wall.fallback("2:3") === wall.fallback("2:3"), "Slots keep their repeated face")
    }

    Component {
        id: surfaceComponent
        Brumm.RenderSurface { width: 1000; height: 700; density: 1 }
    }
    function test_renderSurfaceReusesStorageWhileShrinking() {
        const surface = createTemporaryObject(surfaceComponent, this)
        wait(220)
        const initial = Qt.size(surface.textureSize.width, surface.textureSize.height)
        for (let i = 0; i < 8; ++i) {
            surface.width -= 40
            surface.height -= 30
            wait(16)
            compare(surface.textureSize, initial)
        }
        tryVerify(() => surface.textureSize.width < initial.width, 500)
        verify(surface.textureSize.width >= surface.width)
        verify(surface.textureSize.height >= surface.height)
        surface.width = 1400
        verify(surface.textureSize.width >= 1400, "Growing must allocate immediately")
    }

    function test_smallWallStaysBounded() {
        const wall = createTemporaryObject(wallComponent, this)
        wall.width = 240
        wall.height = 180
        wall.size = 0
        tryCompare(wall, "restW", 240, 500)
        verify(Number.isFinite(wall.cols))
        verify(Number.isFinite(wall.rows))
        verify(wall.cells.length < 100)
    }
}
