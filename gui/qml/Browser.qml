// Browser is the left side: the search box when there is one, and the
// list, floating over the backdrop as the XMB's lists do.
import QtQuick
import QtQuick.Effects

Item {
    id: b

    // The search box on the Search tab, the filter over any list.
    Rectangle {
        id: box
        readonly property bool search: store.section === store.secSearch && store.stack().length === 1
        readonly property bool filter: { store.rev; return !!store.view && store.view.all !== null && store.view.all !== undefined }
        readonly property bool typing: store.searching || store.filtering
        visible: search || filter
        anchors { left: parent.left; right: parent.right; top: parent.top; rightMargin: ui.px(8) }
        height: visible ? ui.px(38) : 0
        radius: height / 2
        color: Qt.alpha(ui.deep, 0.55)
        border { width: 1; color: typing ? Qt.alpha(ui.here, 0.8) : ui.edge }

        Text {
            id: lens
            anchors { left: parent.left; leftMargin: ui.px(14); verticalCenter: parent.verticalCenter }
            text: box.search ? "󰍉" : "󰈲"
            color: box.typing ? ui.here : ui.dim
            font { family: ui.mono; pixelSize: ui.px(15) }
        }
        Text {
            id: typed
            anchors { left: lens.right; leftMargin: ui.px(10); right: parent.right; rightMargin: ui.px(14); verticalCenter: parent.verticalCenter }
            readonly property string value: { store.rev; return box.search ? store.query : (store.view ? store.view.filter || "" : "") }
            text: value || (box.search ? "Songs, albums, artists — or a music.apple.com link" : "Filter this list · tab searches Apple Music")
            color: value ? ui.bright : ui.dim
            font { family: ui.sans; pixelSize: ui.px(14) }
            elide: Text.ElideLeft
            Rectangle { // the cursor, blinking at two frames a second
                visible: box.typing
                x: typed.value ? Math.min(typed.contentWidth, typed.width) + 1 : 0
                anchors.verticalCenter: parent.verticalCenter
                width: 2
                height: ui.px(18)
                radius: 1
                color: ui.here
                Timer {
                    interval: 530
                    repeat: true
                    running: parent.visible && !ui.calm && ui.awake
                    onTriggered: parent.opacity = parent.opacity > 0 ? 0 : 1
                    onRunningChanged: parent.opacity = 1
                }
            }
        }
        TapHandler {
            onTapped: {
                if (box.search) store.searching = true
                else store.filtering = true
                store.rev++
            }
        }
    }

    // The list.
    ListView {
        id: list
        anchors { left: parent.left; right: parent.right; top: box.bottom; bottom: count.top; topMargin: box.visible ? ui.px(12) : 0; bottomMargin: ui.px(4) }
        clip: true
        // It fades out where it runs on past its edges, drawn again only
        // when the list changes.
        layer.enabled: ui.effects
        layer.effect: ShaderEffect {
            property real fadeTop: list.fadeTop / Math.max(1, list.height)
            property real fadeBottom: list.fadeBottom / Math.max(1, list.height)
            fragmentShader: "qrc:/shaders/fade.frag.qsb"
        }
        readonly property real fadeTop: ui.effects && !atYBeginning ? ui.px(36) : 0
        readonly property real fadeBottom: ui.effects && !atYEnd ? ui.px(56) : 0
        // fadeAt is how much of the list shows at y, as its fade has it.
        function fadeAt(y) {
            const ss = (e, t) => { const k = Math.max(0, Math.min(1, t / Math.max(e, 1e-4))); return k * k * (3 - 2 * k) }
            return ss(fadeTop, y) * ss(fadeBottom, height - y)
        }
        // What moves over the list stands here, out of its fade (ListRow).
        readonly property Item overlay: above
        model: store.rows
        currentIndex: store.sel
        highlightFollowsCurrentItem: false
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        cacheBuffer: ui.px(500)
        readonly property int rowH: ui.px(50)
        onHeightChanged: store.pageRows = Math.max(1, Math.floor(height / rowH) - 1)
        Component.onCompleted: store.pageRows = Math.max(1, Math.floor(height / rowH) - 1)

        delegate: ListRow { width: ListView.view.width }

        Connections {
            target: store
            function onSelMoved() { Qt.callLater(() => list.positionViewAtIndex(store.sel, ListView.Contain)) }
            function onRowsChanged() { Qt.callLater(() => list.positionViewAtIndex(store.sel, ListView.Contain)) }
        }
    }

    Item {
        id: above
        x: list.x
        y: list.y
        width: list.width
        height: list.height
        clip: true
    }

    // Where in the list the eye is, while it moves: a glint riding a
    // hairline in the gutter beside it, both running out at their ends.
    Item {
        id: glint
        anchors { left: list.right; leftMargin: ui.px(10); top: list.top; bottom: list.bottom }
        width: 1
        visible: list.contentHeight > list.height && opacity > 0
        opacity: list.moving ? 1 : 0
        Behavior on opacity { enabled: !ui.calm; NumberAnimation { duration: 300 } }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: "transparent" }
                GradientStop { position: 0.5; color: Qt.alpha(ui.bright, 0.12) }
                GradientStop { position: 1; color: "transparent" }
            }
        }
        Item {
            readonly property real along: list.visibleArea.yPosition / Math.max(0.0001, 1 - list.visibleArea.heightRatio)
            y: Math.max(0, Math.min(1, along)) * (glint.height - height)
            width: 1
            height: ui.px(40)
            RectangularShadow {
                visible: ui.effects
                anchors { fill: parent; topMargin: ui.px(8); bottomMargin: ui.px(8) }
                blur: ui.px(6)
                color: Qt.alpha(ui.here, 0.7)
            }
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0; color: "transparent" }
                    GradientStop { position: 0.5; color: Qt.tint(ui.here, Qt.alpha("white", 0.7)) }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
        }
    }

    // What the list says when it has nothing to show.
    Text {
        anchors.centerIn: list
        width: list.width - ui.px(40)
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        visible: text !== ""
        color: ui.dim
        lineHeight: 1.3
        font { family: ui.sans; pixelSize: ui.px(14) }
        text: {
            store.rev
            const v = store.view
            if (!v) return ""
            if (store.st.status === "logged-out") return "Not signed in to Apple Music.\nPress L to sign in."
            if (v.rows.length) return ""
            if (v.loading || (!v.loaded && store.st.status === "starting")) return "Loading…"
            if (v.err) return v.err
            if (v.all !== null && v.all !== undefined) return "Nothing matches.\nTab searches all of Apple Music."
            if (v.key === "search:") return "Type to search Apple Music."
            if (v.key === "queue:") return "Nothing queued."
            if (v.key.startsWith("search:")) return "Nothing found."
            return v.loaded ? "Nothing here yet." : ""
        }
    }

    // How far along the list the selection is.
    Text {
        id: count
        anchors { right: parent.right; bottom: parent.bottom; rightMargin: ui.px(12) }
        color: ui.dim
        font { family: ui.sans; pixelSize: ui.px(11) }
        // Selectable rows up to each row, counted once per list.
        readonly property var upTo: {
            const rows = store.rows, out = new Array(rows.length)
            let n = 0
            for (let i = 0; i < rows.length; i++) {
                if (rows[i].track || rows[i].item) n++
                out[i] = n
            }
            return out
        }
        text: upTo.length && upTo[upTo.length - 1] ? (upTo[Math.min(store.sel, upTo.length - 1)] || 1) + " of " + upTo[upTo.length - 1] : ""
    }
}
