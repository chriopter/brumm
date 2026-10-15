import QtQuick
import ".."

Waves {
    id: fx
    property bool flow: false
    CoverWall {
        visible: !fx.flow && !!store.st.title
        x: fx.coverArea.x
        y: fx.coverArea.y
        width: fx.coverArea.width
        height: fx.coverArea.height
        size: fx.coverSize
        url: win.artwork
    }
    CoverFlow {
        visible: fx.flow && !!store.st.title
        x: fx.coverArea.x
        y: fx.coverArea.y + (fx.coverArea.height - height) / 2
        width: fx.coverArea.width
        size: fx.coverSize
        url: win.artwork
    }
}
