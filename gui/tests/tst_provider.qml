import QtQuick
import QtTest
import "../qml" as Brumm

TestCase {
    name: "ProviderNavigation"
    property bool prototypeMode: false
    property var daemon: fakeDaemon
    property var sys: fakeSystem
    property var appleNavigation: [{"id": "home", "label": "Home", "icon": "home", "destination": {"kind": "home"}}, {"id": "explore", "label": "Explore", "icon": "explore", "destination": {"kind": "page", "route": "app:discover"}}, {"id": "playlists", "label": "Playlists", "icon": "playlists", "destination": {"kind": "library", "list": "playlists"}}, {"id": "albums", "label": "Albums", "icon": "albums", "destination": {"kind": "library", "list": "albums"}}, {"id": "artists", "label": "Artists", "icon": "artists", "destination": {"kind": "library", "list": "artists"}}, {"id": "songs", "label": "Songs", "icon": "songs", "destination": {"kind": "library", "list": "songs"}}, {"id": "radio", "label": "Radio", "icon": "radio", "destination": {"kind": "container", "item": {"kind": "shelf", "id": "radio", "name": "Radio", "catalog": true}}}, {"id": "search", "label": "Search", "icon": "search", "destination": {"kind": "search"}}, {"id": "queue", "label": "Queue", "icon": "queue", "destination": {"kind": "queue"}}]
    property var appleCapabilities: ({collectionKinds:["song","album","playlist"], ratingKinds:["song","album","playlist","station"], favoriteKinds:["song","album","artist","playlist"], ratings:true, dislike:true, favorites:true, collectionWrite:true, playlistWrite:true, folders:true, stations:true, previews:true, autoplay:true})

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
        property var favoriteMembership: ({})
        signal state(var value)
        signal library()
        function send(request) { asked.push(request) }
        function request(request, callback) {
            asked.push(request)
            if (request.cmd === "provider")
                callback("", { provider: { protocolVersion: 1, navigation: appleNavigation, capabilities: appleCapabilities, choices: [{ id: "apple", name: "Apple Music", available: true }, { id: "tidal", name: "Tidal", available: false }], selected: request.query || "", needsChoice: !request.query } })
            else if (request.cmd === "favorites") {
                const states = {}
                for (const ref of request.refs) states[ref.kind + ":" + ref.id] = favoriteMembership[ref.kind + ":" + ref.id] === true
                callback("", {favorites:states})
            } else if (request.cmd === "set-favorite") {
                for (const ref of request.refs) favoriteMembership[ref.kind + ":" + ref.id] = request.saved
                callback("", {})
            }
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
    Component { id: freshStore; Brumm.Store {} }

    function init() { fakeDaemon.asked = []; store.st = { status: "starting" }; store.providerRequested = false; store.configureProvider({ selected: "apple", protocolVersion: 1, navigation: appleNavigation, capabilities: appleCapabilities }) }

    function test_initialStacksAreArraysBeforeProviderReply() {
        const fresh = freshStore.createObject(this)
        verify(Array.isArray(fresh.stack()))
        compare(fresh.stack().length, 1)
        verify(!!fresh.cur())
        verify(Array.isArray(fresh.cur().rows))
        fresh.destroy()
    }

    function test_exploreUsesProviderHandles() {
        store.section = store.secExplore
        const v = store.cur()
        store.loadExploreVisuals(v, { explorer: { title: "Explore", visuals: [
            { name: "Recently played", route: "provider:recent" },
            { name: "Featured", route: "provider:featured" },
            { name: "Videos", route: "provider:videos" }
        ] } })
        const requests = fakeDaemon.asked.filter(r => r.cmd === "explore")
        compare(requests.length, 3)
        compare(requests[0].query, "provider:recent")
        compare(requests[1].query, "provider:featured")
        compare(requests[2].query, "provider:videos")
        compare(fakeDaemon.asked.filter(r => r.cmd === "home").length, 0)
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

    function test_navigationCanReorderRenameAndOmit() {
        const entries = appleNavigation.filter(e => e.id !== "radio").map(e => Object.assign({}, e))
        const artists = entries.splice(entries.findIndex(e => e.id === "artists"), 1)[0]
        artists.label = "My Artists"
        entries.unshift(artists)
        store.section = store.secAlbums
        const place = store.here()
        store.configureProvider({ selected: "apple", protocolVersion: 1, navigation: entries, capabilities: appleCapabilities })
        compare(store.sectionNames[0], "My Artists")
        compare(store.navigation[store.section].id, "albums")
        compare(store.navigation.length, 8)
        compare(store.navigation.filter(e => e.id === "radio").length, 0)
        store.switchTo(store.secQueue)
        store.goPlace(place)
        compare(store.navigation[store.section].id, "albums")
    }

    function test_unsupportedActionsDoNotSendRequests() {
        store.capabilities = {}
        store.st = { status: "ready", id: "song", title: "Song" }
        store.ratePlaying(1)
        store.radioPlaying()
        store.libraryPlaying()
        compare(fakeDaemon.asked.length, 0)
    }

    function test_protocolMismatchIsVisible() {
        const before = store.navigation
        store.configureProvider({ selected: "apple", protocolVersion: 99, navigation: appleNavigation })
        compare(store.navigation, before)
        verify(store.flash.includes("daemon and frontends"))
    }

    function test_newSessionClearsDataForTheSameProvider() {
        store.configureProvider({selected:"apple", generation:1, protocolVersion:1, navigation:appleNavigation, capabilities:appleCapabilities})
        store.st = {status:"ready", title:"Old session"}
        store.recent = [{id:"old-track"}]
        store.upNext = [{id:"old-next"}]
        store.configureProvider({selected:"apple", generation:2, protocolVersion:1, navigation:appleNavigation, capabilities:appleCapabilities})
        compare(store.st.title, undefined)
        compare(store.recent.length, 0)
        compare(store.upNext.length, 0)
    }

    function test_collectionPreferenceUsesDesiredStateAndOneAction() {
        const caps = {favorites:true, favoriteKinds:["song","album","artist","playlist"], playlistWrite:true,
            preference:{label:"Favorite",removeLabel:"Remove from My Collection",collectionEquivalent:true,removable:true}}
        store.configureProvider({selected:"tidal",generation:4,protocolVersion:1,navigation:appleNavigation,capabilities:caps})
        fakeDaemon.favoriteMembership = {}
        fakeDaemon.asked = []
        store.st = {status:"ready",id:"tidal-track",title:"Track"}
        store.ratePlaying(1)
        compare(store.favoriteMark("song","tidal-track"), true)
        compare(fakeDaemon.asked.filter(r => r.cmd === "set-favorite")[0].saved, true)
        store.ratePlaying(1)
        compare(store.favoriteMark("song","tidal-track"), false)
        compare(fakeDaemon.asked.filter(r => r.cmd === "set-favorite")[1].saved, false)
        compare(fakeDaemon.asked.filter(r => r.cmd === "rate" || r.cmd === "add").length, 0)
    }

    function test_providerPickerCanBeOpenedDuringPlayback() {
        store.st = {status:"ready",playing:true}
        store.openProviderPicker()
        compare(store.providerChoiceNeeded,true)
        store.providerKey("esc")
        compare(store.providerPickerOpen,false)
    }

}
