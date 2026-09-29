import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Mpris
import qs.Ui
import qs.Commons

// brumm in the Omarchy bar: the bear and the current title. Click to expand
// a card with the cover, a seek bar, controls and a button that focuses the
// brumm TUI (or opens it). Middle click plays/pauses, scrolling skips.
BarWidget {
  id: root
  moduleName: "chriopter.brumm"

  readonly property var player: {
    const players = Mpris.players ? Mpris.players.values : []
    for (let i = 0; i < players.length; i++) {
      if (players[i].identity === "brumm") return players[i]
    }
    return null
  }
  readonly property bool hasTrack: player !== null && player.trackTitle !== ""
  readonly property bool playing: player !== null && player.isPlaying
  property bool popupOpen: false
  // The title glides when it does not fit, unless brumm's options say not
  // to (o in brumm: "scroll in the bar"); read live from its options file.
  property bool barScroll: true

  function readOptions(text) {
    try {
      root.barScroll = !JSON.parse(text).no_bar_scroll
    } catch (e) {
      root.barScroll = true
    }
  }

  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/brumm/options.json"
    watchChanges: true
    printErrors: false
    onLoaded: root.readOptions(text())
    onFileChanged: reload() // text() is stale in the change signal itself
    onLoadFailed: root.barScroll = true
  }

  function close() { popupOpen = false }
  function clock(sec) {
    const s = Math.max(0, Math.floor(sec))
    return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0")
  }
  function openTui() {
    Quickshell.execDetached(["omarchy-launch-or-focus-tui", "brumm"])
    popupOpen = false
  }

  visible: player !== null
  implicitWidth: visible ? row.implicitWidth + Style.space(14) : 0
  implicitHeight: barSize

  // Quickshell does not push MPRIS position updates; poll while it matters.
  Timer {
    interval: 500
    repeat: true
    running: root.playing && root.popupOpen
    onTriggered: if (root.player) root.player.positionChanged()
  }

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(6)

    Text {
      id: icon
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: root.playing ? "󰝚" : "󰏤"
      color: root.playing ? root.bar.barForeground : Qt.darker(root.bar.barForeground, 1.5)
      font.family: root.bar.fontFamily
      font.pixelSize: Style.font.body
    }

    Item {
      id: clip
      anchors.verticalCenter: parent.verticalCenter
      width: Math.min(Style.space(180), label.implicitWidth)
      height: icon.height
      clip: true
      visible: root.hasTrack && !root.bar.vertical

      Text {
        id: label
        anchors.verticalCenter: parent.verticalCenter
        width: root.barScroll ? implicitWidth : clip.width
        elide: root.barScroll ? Text.ElideNone : Text.ElideRight
        textFormat: Text.PlainText
        text: root.player ? root.player.trackTitle + (root.player.trackArtist ? "  ·  " + root.player.trackArtist : "") : ""
        color: root.bar.barForeground
        font.family: root.bar.fontFamily
        font.pixelSize: Style.font.body

        // Rest, glide slowly to the end, rest, snap back — only when the
        // title does not fit.
        SequentialAnimation on x {
          running: root.barScroll && label.implicitWidth > clip.width && !root.popupOpen
          loops: Animation.Infinite
          onRunningChanged: if (!running) label.x = 0
          PauseAnimation { duration: 2500 }
          NumberAnimation {
            from: 0
            to: clip.width - label.implicitWidth
            duration: Math.max(1, label.implicitWidth - clip.width) * 55
          }
          PauseAnimation { duration: 1800 }
          NumberAnimation { to: 0; duration: 350; easing.type: Easing.OutCubic }
        }
      }
    }
  }

  MouseArea {
    anchors.fill: parent
    cursorShape: Qt.PointingHandCursor
    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
    onClicked: function(mouse) {
      if (!root.player) return
      if (mouse.button === Qt.MiddleButton) root.player.togglePlaying()
      else if (mouse.button === Qt.RightButton) root.openTui()
      else root.popupOpen = !root.popupOpen
    }
    onWheel: function(wheel) {
      if (!root.player) return
      if (wheel.angleDelta.y > 0) root.player.previous()
      else if (wheel.angleDelta.y < 0) root.player.next()
    }
  }

  PopupCard {
    id: popup
    anchorItem: root
    bar: root.bar
    owner: root
    open: root.popupOpen
    contentWidth: popup.fittedContentWidth(Style.space(340))
    contentHeight: popup.fittedContentHeight(column.implicitHeight)

    Column {
      id: column
      anchors.fill: parent
      spacing: Style.space(12)

      Row {
        width: parent.width
        spacing: Style.space(12)

        BorderSurface {
          width: Style.space(112)
          height: Style.space(112)
          radius: Style.spacing.labelGap
          color: Style.normalFillFor(root.bar.foreground, Color.accent)
          borderSpec: Border.controlSpec("normal", root.bar.foreground, Color.accent)

          Image {
            anchors.fill: parent
            anchors.margins: Style.space(2)
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            source: root.player && root.player.trackArtUrl ? root.player.trackArtUrl : ""
            visible: source !== ""
          }
          Text {
            anchors.centerIn: parent
            visible: !root.player || !root.player.trackArtUrl
            text: "ʕ•ᴥ•ʔ"
            color: root.bar.foreground
            font.family: root.bar.fontFamily
            font.pixelSize: Style.font.subtitle
          }
        }

        Column {
          width: parent.width - Style.space(124)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(4)

          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.hasTrack ? root.player.trackTitle : "Nothing playing"
            color: root.bar.foreground
            font.family: root.bar.fontFamily
            font.pixelSize: Style.font.subtitle
            font.bold: true
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.player ? root.player.trackArtist : ""
            visible: text !== ""
            color: Qt.darker(root.bar.foreground, 1.3)
            font.family: root.bar.fontFamily
            font.pixelSize: Style.font.bodySmall
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.player ? root.player.trackAlbum : ""
            visible: text !== ""
            color: Qt.darker(root.bar.foreground, 1.6)
            font.family: root.bar.fontFamily
            font.pixelSize: Style.font.caption
            elide: Text.ElideRight
          }
        }
      }

      Column {
        width: parent.width
        spacing: Style.space(2)
        visible: root.hasTrack && root.player.length > 0

        PanelSlider {
          bar: root.bar
          width: parent.width
          minimum: 0
          maximum: root.player ? Math.max(1, root.player.length) : 1
          value: root.player ? root.player.position : 0
          enabled: root.player && root.player.canSeek
          onReleased: function(v) { if (root.player) root.player.position = v }
        }

        Item {
          width: parent.width
          height: elapsed.implicitHeight
          Text {
            id: elapsed
            anchors.left: parent.left
            text: root.player ? root.clock(root.player.position) : ""
            color: Qt.darker(root.bar.foreground, 1.6)
            font.family: root.bar.fontFamily
            font.pixelSize: Style.font.caption
          }
          Text {
            anchors.right: parent.right
            text: root.player ? "-" + root.clock(root.player.length - root.player.position) : ""
            color: Qt.darker(root.bar.foreground, 1.6)
            font.family: root.bar.fontFamily
            font.pixelSize: Style.font.caption
          }
        }
      }

      // As in brumm: modes on the left, the transport in the middle with the
      // play button the one filled control, the way into brumm on the right.
      Item {
        width: parent.width
        height: transport.implicitHeight

        Row {
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(2)

          Button {
            iconText: "󰒝"
            tooltipText: "Shuffle"
            foreground: root.bar.foreground
            opacity: root.player && root.player.shuffle ? 1 : 0.4
            onClicked: if (root.player) root.player.shuffle = !root.player.shuffle
          }
          Button {
            iconText: root.player && root.player.loopState === MprisLoopState.Track ? "󰑘" : "󰑖"
            tooltipText: "Repeat"
            foreground: root.bar.foreground
            opacity: root.player && root.player.loopState !== MprisLoopState.None ? 1 : 0.4
            onClicked: {
              if (!root.player) return
              const s = root.player.loopState
              root.player.loopState = s === MprisLoopState.None ? MprisLoopState.Playlist
                : s === MprisLoopState.Playlist ? MprisLoopState.Track : MprisLoopState.None
            }
          }
        }

        Row {
          id: transport
          anchors.centerIn: parent
          spacing: Style.space(8)

          Button {
            anchors.verticalCenter: parent.verticalCenter
            iconText: "󰒮"
            foreground: root.bar.foreground
            enabled: root.player && root.player.canGoPrevious
            opacity: enabled ? 1 : 0.4
            onClicked: root.player.previous()
          }
          Button {
            anchors.verticalCenter: parent.verticalCenter
            iconText: root.playing ? "󰏤" : "󰐊"
            iconSize: Style.font.iconLarge
            foreground: root.bar.foreground
            horizontalPadding: Style.spacing.panelGap
            bordered: true
            selected: root.playing
            enabled: root.player !== null
            onClicked: root.player.togglePlaying()
          }
          Button {
            anchors.verticalCenter: parent.verticalCenter
            iconText: "󰒭"
            foreground: root.bar.foreground
            enabled: root.player && root.player.canGoNext
            opacity: enabled ? 1 : 0.4
            onClicked: root.player.next()
          }
        }

        Button {
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          iconText: "󰆍"
          tooltipText: "Open brumm"
          foreground: root.bar.foreground
          onClicked: root.openTui()
        }
      }
    }
  }
}
