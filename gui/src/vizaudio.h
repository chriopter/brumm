// VizAudio is what every visualizer style listens to, as in the terminal
// player (internal/tui/viz.go, vizAudio): the spectrum blended with an
// idle breath while paused, a smoothed copy, falling peaks, band energies
// and a kick detector. It is ticked once a frame while the visualizer
// shows, and hands the bands to the shaders as a small texture
// (VizTexture), updated in place.
#pragma once

#include <QImage>
#include <QObject>
#include <QPointer>
#include <QQuickItem>
#include <QVector>

class VizAudio : public QObject {
    Q_OBJECT
    Q_PROPERTY(double t MEMBER m_t NOTIFY ticked)           // animation seconds
    Q_PROPERTY(double live MEMBER m_live NOTIFY ticked)     // 1 playing … 0 paused, eased
    Q_PROPERTY(double bass MEMBER m_bass NOTIFY ticked)     // smoothed energies, 0–1
    Q_PROPERTY(double mid MEMBER m_mid NOTIFY ticked)
    Q_PROPERTY(double high MEMBER m_high NOTIFY ticked)
    Q_PROPERTY(double level MEMBER m_level NOTIFY ticked)
    Q_PROPERTY(double beat MEMBER m_beat NOTIFY ticked)     // 1 on a kick, decaying over ~0.4 s
    Q_PROPERTY(int kicks MEMBER m_kicks NOTIFY ticked)      // kicks so far
    Q_PROPERTY(double since MEMBER m_since NOTIFY ticked)   // seconds since the last kick
    Q_PROPERTY(int frame MEMBER m_frame NOTIFY ticked)      // counts ticks
    Q_PROPERTY(QVariantList sm READ smList NOTIFY ticked)   // the smoothed bands, for the few styles that want numbers

public:
    static constexpr int Bands = 64;
    static constexpr int WaveN = 256;

    explicit VizAudio(QObject *parent = nullptr);

    // feed takes a frame from the daemon: spectrum 0–255, wave −100–100.
    void feed(const QVector<int> &spectrum, const QVector<int> &wave);
    // tick moves the listener on by dt seconds.
    Q_INVOKABLE void tick(double dt, bool playing);

    // texture is the bands as an image, Bands × 4 pixels:
    //   row 0: spectrum, row 1: smoothed, row 2: peaks — in red, 0–1;
    //   row 3: the waveform, resampled to Bands, 0.5 is silence.
    // A WaveN × 1 image of the waveform alone is waveTexture.
    QImage texture() const { return m_tex; }
    QImage waveTexture() const { return m_waveTex; }
    QVariantList smList() const;

signals:
    void ticked();

private:
    double m_t = 0, m_live = 0, m_bass = 0, m_mid = 0, m_high = 0, m_level = 0, m_beat = 0, m_since = 0;
    double m_avg = 0, m_prev = 0;
    int m_kicks = 0, m_frame = 0;
    QVector<double> m_in, m_wave, m_spec, m_sm, m_peak, m_vel, m_hold;
    QImage m_tex, m_waveTex;
};

// VizTexture is one of VizAudio's textures for the shaders: a texture
// provider, as an Image is, but one texture written anew in place each
// tick, where an Image would make a new one each frame.
//   VizTexture { audio: vizAudio; wave: false } — the bands (64 × 4)
//   VizTexture { audio: vizAudio; wave: true }  — the waveform (256 × 1)
class VizTextureProvider;
class VizTexture : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(VizAudio *audio READ audio WRITE setAudio NOTIFY audioChanged)
    Q_PROPERTY(bool wave MEMBER m_wave NOTIFY waveChanged)

public:
    explicit VizTexture(QQuickItem *parent = nullptr);
    ~VizTexture() override;

    VizAudio *audio() const { return m_audio; }
    void setAudio(VizAudio *a);

    bool isTextureProvider() const override { return true; }
    QSGTextureProvider *textureProvider() const override;

signals:
    void audioChanged();
    void waveChanged();

protected:
    void releaseResources() override;
    void itemChange(ItemChange change, const ItemChangeData &data) override;

private:
    void sync(); // on the render thread, the GUI thread waiting
    void invalidate();
    QImage image() const;

    QPointer<VizAudio> m_audio;
    bool m_wave = false;
    bool m_dirty = true;
    QMetaObject::Connection m_syncing, m_invalidated;
    mutable VizTextureProvider *m_provider = nullptr;
};
