import QtQuick
import QtTest
import "../qml" as Brumm

TestCase {
    name: "ProviderNavigation"
    property bool prototypeMode: false
    property var daemon: fakeDaemon
    property var sys: fakeSystem

    QtObject {
        id: fakeSystem
        function setting(key, fallback) { return fallback }
    }
    QtObject {
        id: fakeDaemon
        property bool connected: false
        property bool omarchy: false
        property var options: ({})
        property var asked: []
        signal state(var value)
        signal library()
        function send(request) { asked.push(request) }
        function request(request, callback) {
            asked.push(request)
            if (request.cmd === "provider")
                callback("", { provider: { choices: [{ id: "apple", name: "Apple Music", available: true }, { id: "spotify", name: "Spotify", available: true, lacks: ["explore", "radio", "preview", "dislike"] }, { id: "tidal", name: "Tidal", available: false }], selected: request.query || "", needsChoice: !request.query } })
            else if (request.cmd === "home")
                callback("", { shelves: [{ title: "Recently played", route: "provider:recent", items: [{ kind: "album", id: "recent", name: "Recent", artwork: "cover" }] }] })
            else if (request.cmd === "explore")
                callback("", { explorer: { items: [{ kind: "album", id: request.query, name: "Result", artwork: "cover" }] } })
            else if (request.cmd === "resolve")
                callback("", { items: [{ kind: "album", id: "resolved", name: "Album", catalog: true }], ids: ["song"] })
            else
                callback("", {})
        }
    }

    Brumm.Store { id: store }

    function init() { fakeDaemon.asked = []; store.st = { status: "starting" }; store.providerRequested = false; store.providerPicking = false; store.flash = "" }

    function test_exploreUsesProviderHandles() {
        store.section = store.secExplore
        const v = store.cur()
        store.loadExploreVisuals(v, { explorer: { title: "Explore", visuals: [
            { name: "Featured", route: "provider:featured" },
            { name: "Videos", route: "provider:videos" }
        ] } })
        const requests = fakeDaemon.asked.filter(r => r.cmd === "explore")
        compare(requests.length, 2)
        compare(requests[0].query, "provider:featured")
        compare(requests[1].query, "provider:videos")
        verify(v.rows.some(r => r.headRoute === "provider:recent"))
        verify(v.rows.some(r => r.head === "Featured" && r.headRoute === "provider:featured"))
    }

    function test_folderParentIsPassedUnchanged() {
        store.section = store.secExplore
        store.stacks[store.secExplore] = [store.newView("Folder", "folder", { kind: "playlist-folders", route: "provider:folder:root" })]
        store.prototypeForm = { route: "action:create:folder" }
        store.submitPrototype("New folder")
        const request = fakeDaemon.asked.find(r => r.cmd === "create")
        verify(request !== undefined)
        compare(request.start, "provider:folder:root")
        compare(request.query, "New folder")
    }

    function test_sharedLinkIsResolvedByProvider() {
        store.query = "https://example.org/music/track"
        store.runSearch(true)
        const request = fakeDaemon.asked.find(r => r.cmd === "resolve")
        verify(request !== undefined)
        compare(request.query, store.query)
        compare(store.section, store.secAlbums)
        compare(store.cur().item.id, "resolved")
    }
    function test_firstRunProviderChoice() {
        store.st = { status: "choose-provider" }
        store.askProviders()
        compare(store.providers.length, 3)
        store.providerKey("down")
        compare(store.providerIndex, 1)
        store.providerKey("down")
        compare(store.providerIndex, 0) // Tidal is skipped
        store.chooseProvider("tidal")
        compare(fakeDaemon.asked.filter(r => r.query === "tidal").length, 0)
        store.providerKey("enter")
        const chosen = fakeDaemon.asked.find(r => r.cmd === "provider" && r.query === "apple")
        verify(chosen !== undefined)
    }

    function test_menuOpensTheServicePicker() {
        store.st = { status: "ready", service: "Apple Music" }
        const i = store.menuRows.findIndex(r => r.act === "service")
        verify(i >= 0)
        compare(store.menuRows[i].label, "Music Service: Apple Music…")
        store.menuRow(i)
        verify(store.providerPicking && store.providerShown)
        store.providerKey("esc")
        verify(!store.providerPicking)
        store.providerPicking = true
        store.providerKey("Q")
        verify(!store.providerPicking) // closes the picker, not the window
        store.pickProvider()
        store.providerKey("down")
        compare(store.providerIndex, 1)
        store.providerKey("enter")
        verify(fakeDaemon.asked.some(r => r.cmd === "provider" && r.query === "spotify"))
        verify(!store.providerPicking)
        verify(store.flash.indexOf("switching to Spotify") >= 0)
    }

    function test_spotifyNamesItselfAndSkipsWhatItLacks() {
        store.st = { status: "ready", service: "Spotify", lacks: ["explore", "radio", "preview", "dislike"] }
        compare(store.service, "Spotify")
        const explore = store.newView("Explore", "explore:", { kind: "shelf", id: "discover", route: "app:discover", catalog: true })
        compare(store.missing(explore), "Explore")
        store.load(explore)
        compare(fakeDaemon.asked.filter(r => r.cmd === "explore").length, 0)
        store.playStation()
        compare(fakeDaemon.asked.filter(r => r.cmd === "station").length, 0)
        verify(store.flash.indexOf("Spotify has no radio") >= 0)
        store.togglePreview()
        compare(fakeDaemon.asked.filter(r => r.cmd === "preview").length, 0)
        store.rate(-1)
        compare(fakeDaemon.asked.filter(r => r.cmd === "rate").length, 0)
        verify(store.flash.indexOf("no dislikes") >= 0)
        store.st = Object.assign({}, store.st, { lacks: store.st.lacks.concat(["love-items"]) })
        store.stacks[store.secPlaylists] = [store.newView("Playlists", "list:playlists")]
        store.switchTo(store.secPlaylists)
        store.cur().rows = [{ item: { kind: "playlist", id: "p", name: "Mine" } }]
        store.cur().sel = 0
        store.rate(1)
        compare(fakeDaemon.asked.filter(r => r.cmd === "rate").length, 0)
        verify(store.flash.indexOf("loves songs only") >= 0)
    }

}
