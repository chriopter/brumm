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
        property var historyPages: ({})
        property var favoriteMembership: ({})
        property string selected: "apple"
        property var configured: ({apple:true})
        property int generation: 1
        function setOption(key, value) { options = Object.assign({}, options, {[key]:value}) }
        signal providerChanged(var info)
        function info() {
            return { protocolVersion:1, generation:generation, selected:selected,
                navigation:selected ? appleNavigation : store.utilityNavigation(),
                capabilities:selected ? appleCapabilities : {},
                needsChoice:!selected, needsSetup:!configured.apple && !configured.tidal && !configured.spotify,
                choices:[{id:"apple",name:"Apple Music",available:true,configured:!!configured.apple},
                         {id:"tidal",name:"Tidal",available:true,configured:!!configured.tidal},
                         {id:"spotify",name:"Spotify",available:true,configured:!!configured.spotify}] }
        }
        signal state(var value)
        signal library()
        function send(request) {
            asked.push(request)
            if (request.cmd === "login") {
                configured = Object.assign({}, configured, {[selected]:true}); generation++
                providerChanged(info()); state({status:"ready"})
            }
        }
        function request(request, callback) {
            asked.push(request)
            if (request.cmd === "options") {
                options = Object.assign({}, options, request.options || {})
                callback("", {})
            } else if (request.cmd === "provider") {
                if (request.query) { selected = request.query; generation++ }
                callback("", {provider:info()})
            } else if (request.cmd === "favorites") {
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
            else if (request.cmd === "history")
                callback("", historyPages[request.query || ""] || {})
            else if (request.cmd === "queue")
                callback("", {tracks:[{id:"previous",title:"Previous"},{id:"playing",title:"Playing"},{id:"next",title:"Next"}],pos:1})
            else
                callback("", {})
        }
    }

    Brumm.Store { id: store }
    Component { id: freshStore; Brumm.Store {} }

    function initTestCase() {
        // Exercise the untouched Store before init configures an Apple session.
        verify(Array.isArray(store.stacks[0]))
        fakeDaemon.selected = ""; fakeDaemon.configured = {}
        store.st = { status: "choose-provider" }
        store.askProviders()
        compare(store.selectedProvider, "")
        compare(store.providers.length, 3)
        compare(store.cur().key, "search:")
        store.providerKey("enter")
        compare(store.selectedProvider, "apple")
        compare(store.cur().key, "home:")
    }

    function init() {
        fakeDaemon.asked = []; fakeDaemon.historyPages = {}; store.recent = []; fakeDaemon.selected = "apple"; fakeDaemon.configured = {apple:true}
        store.st = {status:"starting"}; store.providerRequested = false
        store.configureProvider(fakeDaemon.info())
        store.closeVisualizer()
        store.setupMode = false; store.providerPickerOpen = false; store.connectingProvider = ""
    }

    function test_sharedVisualizerUpdatesDoNotEcho() {
        fakeDaemon.options = {viz:"synth", viz_full:false, viz_fps:120}
        compare(store.vizNames[store.vizStyle], "synth")
        verify(!store.full)
        const before = fakeDaemon.asked.length
        fakeDaemon.options = {viz:"wire", viz_full:true, viz_fps:60}
        compare(store.vizNames[store.vizStyle], "wire")
        verify(store.full)
        compare(fakeDaemon.asked.length, before)
        store.full = false
        compare(fakeDaemon.options.viz_full, false)
        store.vizShow(0, true)
        compare(fakeDaemon.options.viz, "coverwall")
    }

    function test_inlineVisualizerSurvivesFrontendHandoff() {
        store.goPlace({sectionId:"home", full:false, visualizer:"synth", views:[], sel:{}})
        compare(store.vizNames[store.vizStyle], "synth")
        verify(!store.full)
        compare(store.here().visualizer, "synth")
    }

    function test_visualizerHoverDoesNotChangeChosenStyle() {
        store.vizStyle = 0
        const saved = daemon.options.viz
        store.vizHover = 5
        compare(store.vizDisplayed, 5)
        compare(store.vizStyle, 0)
        compare(daemon.options.viz, saved)
        store.vizHover = -1
        compare(store.vizDisplayed, 0)
    }

    function test_playlistSortingKeepsOriginalOrderAndSurvivesReload() {
        store.section = store.secPlaylists
        const v = store.newView("Playlist", "playlist:test", {kind:"playlist", id:"test"})
        store.stacks[store.section].push(v)
        const tracks = [{id:"b",title:"Bravo",duration:90}, {id:"a",title:"Alpha",duration:120}]
        store.fill(v, {tracks:tracks})
        store.sortPlaylist("title")
        compare(v.rows[0].track.id, "a")
        store.fill(v, {tracks:tracks.concat([{id:"c",title:"Charlie",duration:30}])})
        compare(v.rows[0].track.id, "a")
        store.sortPlaylist("duration")
        compare(v.rows[0].track.id, "c")
        store.sortPlaylist("original")
        compare(v.rows.map(r => r.track.id).join(","), "b,a,c")
    }

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
    function test_firstRunCanConnectAllServices() {
        fakeDaemon.selected = ""; fakeDaemon.configured = {}; fakeDaemon.generation++
        store.configureProvider(fakeDaemon.info())
        store.st = {status:"choose-provider"}
        verify(store.needsSetup)
        store.connectProvider("apple")
        verify(store.providerChoiceNeeded)
        compare(store.connectedProviders.length, 1)
        store.connectProvider("tidal")
        compare(store.connectedProviders.length, 2)
        verify(fakeDaemon.configured.apple)
        verify(fakeDaemon.configured.tidal)
        store.connectProvider("spotify")
        compare(store.connectedProviders.length, 3)
        verify(fakeDaemon.configured.spotify)
        store.finishServices()
        verify(!store.providerChoiceNeeded)
        store.openProviderPicker()
        store.chooseProvider("apple", false)
        compare(store.selectedProvider, "apple")
        compare(store.connectedProviders.length, 3)
    }

    function test_navigationCanReorderRenameAndOmit() {
        const entries = appleNavigation.filter(e => e.id !== "radio").map(e => Object.assign({}, e))
        const artists = entries.splice(entries.findIndex(e => e.id === "artists"), 1)[0]
        artists.label = "My Artists"
        entries.unshift(artists)
        store.section = store.secAlbums
        const place = store.here()
        store.configureProvider({ selected: "apple", generation:store.providerGeneration, protocolVersion: 1, navigation: entries, capabilities: appleCapabilities })
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
        store.cur().rows = [{track:{id:"song",title:"Song"}}]
        store.cur().sel = 0
        store.spaceDown = true
        store.hold()
        store.togglePreview()
        store.spaceDown = false
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

    function test_servicesStayInOptionsSectionWhenPreferencesAreCollapsed() {
        store.menuOptions = false
        const rows = store.menuRows
        const heading = rows.findIndex(r => r.heading === "Options")
        const services = rows.findIndex(r => r.act === "ctrl+o")
        const preferences = rows.findIndex(r => r.fold)
        verify(heading >= 0 && services > heading && services < preferences)
        compare(rows.filter(r => r.option !== undefined).length, 0)
        store.menuRow(services)
        verify(store.providerPickerOpen)
        store.providerKey("esc")
    }

    function test_nowPlayingShowsFreshQueueAndSelectsPlayingSong() {
        store.switchTo(store.secAlbums)
        store.st = {status:"ready",id:"playing",source:"album:current"}
        fakeDaemon.asked = []
        store.compactBrowser = true
        store.showPlayer()
        verify(store.playerView)
        verify(!store.compactBrowser)
        compare(store.section,store.secQueue)
        compare(store.cur().key,"queue:")
        compare(store.cur().rows.length,3)
        compare(store.cur().sel,1)
        compare(fakeDaemon.asked.filter(r => r.cmd === "queue").length,1)
        store.showPlayer()
        compare(fakeDaemon.asked.filter(r => r.cmd === "queue").length,2)
        store.switchTo(store.secHome)
        verify(!store.playerView)
        compare(store.section,store.secHome)
    }

    function test_historyIsVisibleWithoutAnUpcomingQueue() {
        const view = store.cur()
        view.key = "queue:"
        store.st = {status:"ready"}
        fakeDaemon.historyPages = {"": {tracks:[{id:"heard",title:"Already played"}],next:""}}
        store.fill(view, {tracks:[],pos:0})
        store.loadHistory(view, true)
        compare(view.rows.length, 1)
        compare(view.rows[0].track.id, "heard")
        verify(view.rows[0].played)
        compare(view.sel, 0)
    }

    function test_historyPrependsWithoutChangingQueuePositions() {
        store.st = {status:"ready",id:"playing",title:"Playing"}
        fakeDaemon.historyPages = {
            "": {tracks:[{id:"playing",title:"Playing"},{id:"older",title:"Older"}],next:"page2"},
            "page2": {tracks:[{id:"oldest",title:"Oldest"}],next:""}
        }
        store.showPlayer()
        wait(0)
        const view = store.cur()
        compare(view.rows[0].track.id,"older")
        compare(view.rows[view.sel].track.id,"playing")
        store.loadHistory(view, false)
        compare(view.rows[0].track.id,"oldest")
        compare(view.rows[1].track.id,"older")
        compare(view.rows[view.sel].track.id,"playing")
        const nextIndex = view.rows.findIndex(r => r.track.id === "next")
        const expectedPosition = view.rows[nextIndex].queueIndex
        store.activateAt(nextIndex)
        const jump = fakeDaemon.asked.filter(r => r.cmd === "jump").pop()
        compare(jump.value,expectedPosition)
        const requests = fakeDaemon.asked.filter(r => r.cmd === "history").length
        store.loadHistory(view, false)
        compare(fakeDaemon.asked.filter(r => r.cmd === "history").length,requests)
    }

    function test_visualizerSurvivesFrontendHandoff() {
        store.full = true
        store.vizStyle = store.vizNames.indexOf("wire")
        const place = store.here()
        compare(place.full,true)
        compare(place.visualizer,"wire")
        store.full = false
        store.vizStyle = 0
        store.goPlace(place)
        verify(store.full)
        compare(store.vizNames[store.vizStyle],"wire")
        store.vizList = true
        compare(store.vizKey("g"),false)
        store.full = false
        store.vizList = false
    }

    function test_visualizerDefaultsAndPlayerModes() {
        compare(store.vizNames.slice(0, 3).join(","), "coverwall,coverflow,waves")
        compare(store.vizNames[store.vizStyle], "coverwall")
        verify(store.vizNames.includes("waves"))
        verify(store.inlineViz)
        store.key("f")
        verify(store.full)
        compare(store.vizNames[store.vizStyle], "coverwall")
        store.vizList = true
        verify(store.vizKey("f"))
        verify(store.inlineViz)
        verify(!store.vizList)
        store.vizShow(store.vizNames.indexOf("waves"), true)
        store.key("f")
        verify(store.full)
        compare(store.vizNames[store.vizStyle], "waves")
        store.vizKey("esc")
        verify(!store.full)
        compare(store.vizNames[store.vizStyle], "coverwall")
    }

    function test_albumActionsTargetAlbumInsteadOfSelectedSong() {
        const album = {kind:"album",id:"album-id",name:"An album",catalog:true}
        const view = store.cur()
        view.rows = [{track:{id:"selected-song",title:"A song"}}]
        view.sel = 0
        store.addToLibrary({item:album})
        const add = fakeDaemon.asked.find(r => r.cmd === "add")
        compare(add.list, "albums")
        compare(add.ids[0], "album-id")
        store.openPicker({item:album})
        compare(store.pick.what.item.id, "album-id")
        compare(store.pick.name, "An album")
        store.pick = null
        store.rate(1, {item:album})
        const rating = fakeDaemon.asked.find(r => r.cmd === "rate")
        compare(rating.refs[0].kind, "album")
        compare(rating.refs[0].id, "album-id")
    }

    function test_visualizerFrameRatesUseSharedOptions() {
        fakeDaemon.options = {}
        compare(store.vizRate(144),30)
        for (const expected of [60,120,-1,0]) {
            verify(store.vizKey("ctrl+f"))
            compare(fakeDaemon.options.viz_fps,expected)
        }
        fakeDaemon.options = {viz_fps:-1}
        compare(store.vizRate(144),144)
        compare(store.vizRateLabel(144),"Auto · 144 FPS")
        fakeDaemon.options = {viz_fps:17}
        compare(store.vizRate(144),30)
        fakeDaemon.options = {}
    }

}
