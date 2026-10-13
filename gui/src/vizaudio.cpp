#include "vizaudio.h"

#include <QQuickWindow>
#include <QRunnable>
#include <QSGTextureProvider>

#include <QtMath>
#include <algorithm>
#include <cmath>

VizAudio::VizAudio(QObject *parent)
    : QObject(parent), m_listener(brumm_listener_new()), m_in(Bands, 0), m_wave(WaveN, 0),
      m_tex(Bands, 4, QImage::Format_RGBA8888), m_waveTex(WaveN, 1, QImage::Format_RGBA8888)
{
    m_tex.fill(Qt::black);
    m_waveTex.fill(QColor(128, 0, 0));
}

VizAudio::~VizAudio() { brumm_listener_free(m_listener); }

void VizAudio::feed(const QVector<int> &spectrum, const QVector<int> &wave)
{
    if (!spectrum.isEmpty())
        for (int i = 0; i < Bands; i++)
            m_in[i] = std::clamp(spectrum.value(i * spectrum.size() / Bands) / 255.0, 0.0, 1.0);
    if (!wave.isEmpty())
        for (int i = 0; i < WaveN; i++)
            m_wave[i] = std::clamp(wave.value(i * wave.size() / WaveN) / 100.0, -1.0, 1.0);
}

void VizAudio::tick(double dt, bool playing)
{
    brumm_listener_tick(m_listener, m_in.constData(), m_in.size(), dt, playing);
    BrummLevels levels{};
    brumm_listener_levels(m_listener, &levels);
    m_t = levels.t;
    m_live = levels.live;
    m_bass = levels.bass;
    m_mid = levels.mid;
    m_high = levels.high;
    m_level = levels.level;
    m_beat = levels.beat;
    m_since = levels.since;
    m_kicks = levels.kicks;
    std::size_t ns = 0, nsm = 0, np = 0;
    const double *spec = brumm_listener_bands(m_listener, Spectrum, &ns);
    const double *sm = brumm_listener_bands(m_listener, Smoothed, &nsm);
    const double *peak = brumm_listener_bands(m_listener, Peaks, &np);
    // The listener has as many bands as it was fed (m_in's); anything else
    // would read past them.
    constexpr std::size_t n = Bands;
    if (!spec || !sm || !peak || ns < n || nsm < n || np < n)
        return;

    auto px = [](double v) { return uchar(std::clamp(v, 0.0, 1.0) * 255); };
    for (int i = 0; i < Bands; i++) {
        uchar *r0 = m_tex.scanLine(0) + i * 4, *r1 = m_tex.scanLine(1) + i * 4, *r2 = m_tex.scanLine(2) + i * 4,
              *r3 = m_tex.scanLine(3) + i * 4;
        r0[0] = px(spec[i]), r1[0] = px(sm[i]), r2[0] = px(peak[i]);
        r3[0] = px(0.5 + 0.5 * m_wave[i * WaveN / Bands] * m_live);
        r0[3] = r1[3] = r2[3] = r3[3] = 255;
    }
    for (int i = 0; i < WaveN; i++) {
        uchar *p = m_waveTex.scanLine(0) + i * 4;
        p[0] = px(0.5 + 0.5 * m_wave[i] * m_live);
        p[3] = 255;
    }
    m_frame++;
    emit ticked();
}

QVariantList VizAudio::smList() const
{
    QVariantList out;
    out.reserve(Bands);
    std::size_t count = 0;
    const double *sm = brumm_listener_bands(m_listener, Smoothed, &count);
    for (int i = 0; i < Bands; ++i)
        out << (i < int(count) ? sm[i] : 0.0);
    return out;
}

// ── the texture ────────────────────────────────────────────────────────

// The texture the shaders read: made anew from the image each tick (it
// is 64 × 4 pixels, or 256 × 1), outside Qt's shared atlas, so every
// shader samples it as a plain texture. Public API only: brumm-gui keeps
// working across Qt updates.
class VizTextureProvider : public QSGTextureProvider {
public:
    explicit VizTextureProvider(QQuickWindow *w) : m_window(w) {}
    ~VizTextureProvider() override { delete m_texture; }
    QSGTexture *texture() const override { return m_texture; }
    // set makes the texture from img; on the render thread.
    void set(const QImage &img)
    {
        if (img.isNull() || !m_window)
            return;
        QSGTexture *t = m_window->createTextureFromImage(img);
        if (!t)
            return;
        t->setFiltering(QSGTexture::Linear);
        t->setHorizontalWrapMode(QSGTexture::ClampToEdge);
        t->setVerticalWrapMode(QSGTexture::ClampToEdge);
        delete m_texture;
        m_texture = t;
        emit textureChanged();
    }

private:
    QPointer<QQuickWindow> m_window;
    QSGTexture *m_texture = nullptr;
};

namespace {
// Deletes a provider on the render thread, between frames.
class Cleanup : public QRunnable {
public:
    explicit Cleanup(VizTextureProvider *p) : m_p(p) {}
    void run() override { delete m_p; }

private:
    VizTextureProvider *m_p;
};
} // namespace

VizTexture::VizTexture(QQuickItem *parent) : QQuickItem(parent) {}

VizTexture::~VizTexture()
{
    // Out of the window, releaseResources has already let it go.
    if (m_provider && window())
        window()->scheduleRenderJob(new Cleanup(m_provider), QQuickWindow::NoStage);
}

void VizTexture::setAudio(VizAudio *a)
{
    if (m_audio == a)
        return;
    if (m_audio)
        disconnect(m_audio, nullptr, this, nullptr);
    m_audio = a;
    if (a)
        connect(a, &VizAudio::ticked, this, [this] { m_dirty = true; });
    m_dirty = true;
    emit audioChanged();
}

QImage VizTexture::image() const
{
    if (!m_audio)
        return {};
    return m_wave ? m_audio->waveTexture() : m_audio->texture();
}

// Called on the render thread while the GUI thread waits (the shaders
// ask for it as they are synced).
QSGTextureProvider *VizTexture::textureProvider() const
{
    if (!m_provider) {
        m_provider = new VizTextureProvider(window());
        m_provider->set(image());
    }
    return m_provider;
}

void VizTexture::sync()
{
    if (!m_provider || !m_dirty)
        return;
    m_dirty = false;
    m_provider->set(image());
}

void VizTexture::invalidate()
{
    delete m_provider;
    m_provider = nullptr;
}

void VizTexture::releaseResources()
{
    if (m_provider && window()) {
        window()->scheduleRenderJob(new Cleanup(m_provider), QQuickWindow::NoStage);
        m_provider = nullptr;
    }
}

void VizTexture::itemChange(ItemChange change, const ItemChangeData &data)
{
    if (change == ItemSceneChange) {
        disconnect(m_syncing);
        disconnect(m_invalidated);
        if (QQuickWindow *w = data.window) {
            // Fresh pixels go in as each frame is synced, the GUI thread
            // waiting: nothing is drawn for them that is not drawn anyway.
            m_syncing = connect(w, &QQuickWindow::beforeSynchronizing, this, &VizTexture::sync, Qt::DirectConnection);
            m_invalidated = connect(w, &QQuickWindow::sceneGraphInvalidated, this, &VizTexture::invalidate, Qt::DirectConnection);
        }
    }
    QQuickItem::itemChange(change, data);
}
