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
                callback("", { provider: { choices: [{ id: "apple", name: "Apple Music", available: true }, { id: "tidal", name: "Tidal", available: false }], selected: request.query || "", needsChoice: !request.query } })
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

    function init() { fakeDaemon.asked = []; store.st = { status: "starting" }; store.providerRequested = false }

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
        compare(store.providers.length, 2)
        store.providerKey("down")
        compare(store.providerIndex, 0)
        store.chooseProvider("tidal")
        compare(fakeDaemon.asked.filter(r => r.query === "tidal").length, 0)
        store.providerKey("enter")
        const chosen = fakeDaemon.asked.find(r => r.cmd === "provider" && r.query === "apple")
        verify(chosen !== undefined)
    }

}
