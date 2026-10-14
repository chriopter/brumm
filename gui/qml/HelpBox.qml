// HelpBox is ?: every key, in groups.
import QtQuick

Box {
    title: "keys"
    boxWidth: ui.px(780)

    // The terminal player's list (crates/brumm/src/tui/view.rs help_lines), in the
    // same groups and order; only the window's own keys differ.
    readonly property var groups: [
        ["Browse", [["↑↓  j k", "move"], ["enter  l  →", "open · play"], ["esc  h", "back"], ["←  then ↑↓", "the sections"],
                    ["tab  1 – " + store.secQueue, "next section · a section"], ["home  G", "top · bottom"], ["/", "filter; tab: all of " + store.providerName],
                    ["a  A", "the song's album · artist"], ["c", "go to what plays"]]],
        ["Play", [["space", "play · pause"], (store.capabilities.previews ? ["hold space  O", "preview the song"] : null), ["n  p", "next · back (p restarts after 3 s)"],
                  (store.capabilities.stations ? ["R", "radio from the song or artist"] : null), ["z  Z", "add to queue · play next"], ["⇧←  ⇧→", "seek 10 s"],
                  ["s", "shuffle"], ["r", "repeat off · all · one"], ["+  -  m", "volume · mute"]]],
        ["Library", [(store.capabilities.preference ? ["*", "favorite"] : (store.capabilities.ratings ? ["*  d", "love · dislike"] : null)), (store.capabilities.collectionWrite ? ["i", "add to your library"] : null), ["P", "add to a playlist, or a new one"], ["y", "copy the song's link"]]],
        ["brumm", [["?", "this list"], ["o", "options"], ["ctrl+o", "switch music provider"], ["f", "visualizer (tab, v styles, a auto)"], ["F", "visualizer full screen"], ["[  ]", "list narrower · wider"],
                   ["g", "to the terminal player"], ["Q", "close, music plays on"], ["q", "quit, music stops"], ["L", "sign in again"],
                   ["U", "look for an update, install it"], ["!", "send feedback: a new issue on GitHub"]]],
    ]

    Grid {
        columns: 2
        columnSpacing: ui.px(40)
        rowSpacing: ui.px(20)
        Repeater {
            model: groups
            Column {
                required property var modelData
                spacing: ui.px(5)
                width: ui.px(330)
                Text { text: modelData[0]; color: ui.bright; font { family: ui.sans; pixelSize: ui.px(13); weight: Font.DemiBold } }
                Repeater {
                    model: modelData[1].filter(function (row) { return row !== null })
                    Row {
                        required property var modelData
                        Text { width: ui.px(110); text: modelData[0]; color: ui.here; font { family: ui.sans; pixelSize: ui.px(12); weight: Font.DemiBold } }
                        Text { text: modelData[1]; color: ui.fg; font { family: ui.sans; pixelSize: ui.px(12) } }
                    }
                }
            }
        }
    }
    Item { width: 1; height: ui.px(6) }
    KeyHints { pairs: [["esc", "close"]] }
}
