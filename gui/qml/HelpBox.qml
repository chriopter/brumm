// HelpBox is ?: every key, in groups.
import QtQuick

Box {
    title: "keys"
    boxWidth: ui.px(780)

    // The terminal player's list (internal/tui/view.go helpLines), in the
    // same groups and order; only the window's own keys differ.
    readonly property var groups: [
        ["Browse", [["↑↓  j k", "move"], ["enter  l  →", "open · play"], ["esc  h", "back"], ["←  then ↑↓", "the sections"],
                    ["tab  1 – 8", "next section · a section"], ["home  G", "top · bottom"], ["/", "filter; tab: all of Apple Music"],
                    ["a  A", "the song's album · artist"], ["c", "go to what plays"]]],
        ["Play", [["space", "play · pause"], ["hold space  O", "preview the song"], ["n  p", "next · back (p restarts after 3 s)"],
                  ["R", "radio from the song or artist"], ["z  Z", "add to queue · play next"], ["⇧←  ⇧→", "seek 10 s"],
                  ["s", "shuffle"], ["r", "repeat off · all · one"], ["+  -  m", "volume · mute"]]],
        ["Library", [["*  d", "love · dislike"], ["i", "add to your library"], ["P", "add to a playlist, or a new one"], ["y", "copy the song's link"]]],
        ["brumm", [["?", "this list"], ["o", "options"], ["f", "visualizer (tab, v styles, a auto)"], ["[  ]", "list narrower · wider"],
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
                    model: modelData[1]
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
