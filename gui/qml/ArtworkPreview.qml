import QtQuick

Item {
    id: preview
    property bool flow: false
    clip: true
    readonly property var artwork: {
        store.rev
        const urls = [], seen = new Set()
        const add = url => { if (url && !seen.has(url) && urls.length < 9) { seen.add(url); urls.push(url) } }
        add(win.artwork)
        for (const track of store.upNext) add(track.artwork)
        for (const track of store.recent) add(track.artwork)
        for (const stack of store.stacks)
            for (const view of stack)
                for (const row of view.rows) add((row.item || row.track || {}).artwork)
        return urls
    }
    Backdrop { anchors.fill: parent }
    Repeater {
        model: preview.flow ? 5 : 9
        Image {
            id: mini
            required property int index
            readonly property int offset: index - 2
            width: preview.flow ? preview.width * (offset === 0 ? 0.55 : 0.43) : preview.width / 3 - 2
            height: width
            x: preview.flow ? preview.width / 2 + offset * preview.width * 0.2 - width / 2 : (index % 3) * preview.width / 3 + 1
            y: preview.flow ? (preview.height - height) / 2 : Math.floor(index / 3) * preview.height / 3 + 1
            z: preview.flow ? 5 - Math.abs(offset) : 0
            opacity: preview.flow && offset !== 0 ? 0.75 : 0.65
            source: preview.artwork.length ? "image://cover/" + encodeURIComponent(preview.artwork[(preview.flow ? Math.abs(offset) : index) % preview.artwork.length]) : ""
            sourceSize: Qt.size(96, 96)
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            transform: Rotation {
                origin.x: mini.width / 2
                origin.y: mini.height / 2
                axis { x: 0; y: 1; z: 0 }
                angle: preview.flow ? Math.sign(offset) * -55 : 0
            }
        }
    }
    Image {
        visible: !preview.flow
        anchors.centerIn: parent
        width: preview.width * 0.48
        height: width
        source: win.artwork ? "image://cover/" + encodeURIComponent(win.artwork) : ""
        sourceSize: Qt.size(96, 96)
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
}
