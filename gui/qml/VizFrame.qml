import QtQuick

QtObject {
    id: frame
    property bool running: false
    readonly property real frameTime: vizClock.frameTime
    signal triggered()
    property Connections connection: Connections {
        target: vizClock
        enabled: frame.running
        function onTriggered() { frame.triggered() }
    }
}
