import QtQuick
import QtMultimedia

Item {
    id: player
    property string sourceUrl: ""
    property string errorText: ""
    MediaPlayer {
        id: media
        autoPlay: true
        source: player.sourceUrl
        videoOutput: output
        audioOutput: AudioOutput { volume: store.st.volume === undefined ? 0.7 : store.st.volume }
        onErrorOccurred: player.errorText = errorString
    }
    VideoOutput { id: output; anchors.fill: parent; fillMode: VideoOutput.PreserveAspectFit }
    onSourceUrlChanged: { errorText = ""; if (sourceUrl) media.play(); else media.stop() }
    Component.onDestruction: media.stop()
    Text { anchors.centerIn: parent; width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; text: player.errorText; color: ui.fg; visible: !!player.errorText }
    function toggle() { if (media.playbackState === MediaPlayer.PlayingState) media.pause(); else media.play() }
}
