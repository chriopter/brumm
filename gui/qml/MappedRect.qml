// Map a live item rectangle, including moves of any ancestor during layout.
import QtQuick

Item {
    property Item sourceItem: null
    property Item targetItem: null
    readonly property rect bounds: {
        if (!sourceItem || !targetItem) return Qt.rect(0, 0, 0, 0)
        // mapToItem alone does not subscribe to ancestor geometry changes.
        let geometry = 0
        for (let node = sourceItem; node; node = node.parent)
            geometry += node.x + node.y + node.width + node.height + node.scale + node.rotation
        for (let node = targetItem; node; node = node.parent)
            geometry += node.x + node.y + node.width + node.height + node.scale + node.rotation
        if (!isFinite(geometry)) return Qt.rect(0, 0, 0, 0)
        return sourceItem.mapToItem(targetItem, Qt.rect(0, 0, sourceItem.width, sourceItem.height))
    }
}
