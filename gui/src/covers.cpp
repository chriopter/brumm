#include "covers.h"

#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QFile>
#include <QImageReader>
#include <QNetworkReply>
#include <QMutex>
#include <QSaveFile>
#include <QThread>
#include <memory>
#include <QStandardPaths>
#include <QThreadPool>
#include <cmath>

// The covers the terminal player keeps (internal/art): the same folder,
// the same names, so what one has fetched the other shows at once.
static QString cacheDir()
{
    QString d = qEnvironmentVariable("XDG_CACHE_HOME");
    if (d.isEmpty())
        d = QDir::homePath() + "/.cache";
    return d + "/brumm/covers";
}

static QString cachePath(const QString &url)
{
    const QByteArray sum = QCryptographicHash::hash(url.toUtf8(), QCryptographicHash::Sha256);
    return cacheDir() + "/" + QString::fromLatin1(sum.left(12).toHex());
}

Covers::Covers(QObject *parent) : QObject(parent)
{
    m_net.setTransferTimeout(15000);
    // Keep the cache small: the least recently used covers go first, as
    // the terminal player prunes it (art.Prune, 200 MB).
    QThreadPool::globalInstance()->start([] {
        QDir dir(cacheDir());
        const auto files = dir.entryInfoList(QDir::Files, QDir::Time); // newest first
        qint64 total = 0;
        for (const QFileInfo &f : files)
            if ((total += f.size()) > (200 << 20))
                QFile::remove(f.filePath());
    });
}

// cached reads the cover at url from the cache, if it is there, decoded
// at no more than size (JPEG decodes smaller for less), and marks it used.
// Any thread.
static QImage cached(const QString &url, QSize size = {})
{
    const QString path = cachePath(url);
    QImageReader reader(path);
    if (!reader.canRead())
        return {};
    const QSize full = reader.size();
    if (size.isValid() && size.width() > 0 && full.isValid() && size.width() < full.width())
        reader.setScaledSize(full.scaled(size, Qt::KeepAspectRatio));
    QImage img = reader.read();
    if (!img.isNull())
        QFile(path).setFileTime(QDateTime::currentDateTime(), QFileDevice::FileModificationTime); // used: kept longest
    return img;
}

void Covers::image(const QString &url, std::function<void(QImage)> done)
{
    Q_ASSERT(thread() == QThread::currentThread()); // the network lives here
    const QString path = cachePath(url);
    if (QFile::exists(path)) {
        QThreadPool::globalInstance()->start([url, path, done, self = QPointer<Covers>(this)] {
            const QImage img = cached(url);
            QMetaObject::invokeMethod(self, [self, url, path, done, img] {
                if (!img.isNull())
                    return done(img);
                QFile::remove(path); // unreadable: fetch it anew
                self->image(url, done);
            });
        });
        return;
    }
    auto &waiting = m_waiting[url];
    waiting.append(done);
    if (waiting.size() > 1)
        return; // on its way already
    QNetworkReply *reply = m_net.get(QNetworkRequest(QUrl(url)));
    // The reply is the context: the slot never runs once it is gone.
    connect(reply, &QNetworkReply::finished, reply, [this, reply, url, path] {
        const QByteArray data = reply->error() == QNetworkReply::NoError ? reply->readAll() : QByteArray();
        reply->deleteLater();
        QThreadPool::globalInstance()->start([this, data, url, path, self = QPointer<Covers>(this)] {
            QImage img = QImage::fromData(data);
            if (!img.isNull()) {
                QDir().mkpath(cacheDir());
                QSaveFile f(path);
                if (f.open(QIODevice::WriteOnly)) {
                    f.write(data);
                    f.commit();
                }
            }
            QMetaObject::invokeMethod(self, [this, url, img] {
                const auto done = m_waiting.take(url);
                for (const auto &d : done)
                    d(img);
            });
        });
    });
}

// ── the image provider ─────────────────────────────────────────────────

namespace {

// A response lives in Qt Quick's image loading thread, while covers load
// on the main thread: they meet in a guarded link that the response cuts
// when it goes, so a late cover finds no one rather than a dead object.
class Response;
struct Link {
    QMutex mu;
    Response *to = nullptr;
};

class Response : public QQuickImageResponse {
public:
    Response(Covers *covers, const QString &url, QSize size) : m_link(std::make_shared<Link>())
    {
        m_link->to = this;
        auto deliver = [link = m_link, size](QImage img) {
            if (!img.isNull() && size.isValid() && size.width() > 0 && size.width() < img.width())
                img = img.scaled(size, Qt::KeepAspectRatio, Qt::SmoothTransformation);
            QMutexLocker lock(&link->mu);
            if (!link->to)
                return; // no longer wanted
            link->to->m_img = img;
            emit link->to->finished(); // allowed from any thread
        };
        // A cover in the cache is read straight away, at the size shown,
        // off the main thread: it is there in a frame or two. Only one to
        // fetch goes by way of the covers, where the network is.
        if (QFile::exists(cachePath(url))) {
            QThreadPool::globalInstance()->start([covers = QPointer<Covers>(covers), url, size, deliver] {
                const QImage img = cached(url, size);
                if (!img.isNull())
                    return deliver(img);
                if (covers) // unreadable: fetch it again
                    QMetaObject::invokeMethod(covers, [covers, url, deliver] { covers->image(url, deliver); });
            });
            return;
        }
        QMetaObject::invokeMethod(covers, [covers, url, deliver] { covers->image(url, deliver); }, Qt::QueuedConnection);
    }
    ~Response() override
    {
        QMutexLocker lock(&m_link->mu);
        m_link->to = nullptr;
    }
    QQuickTextureFactory *textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_img); }

private:
    std::shared_ptr<Link> m_link;
    QImage m_img;
};

class Provider : public QQuickAsyncImageProvider {
public:
    explicit Provider(Covers *c) : m_covers(c) {}
    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &size) override
    {
        return new Response(m_covers, QUrl::fromPercentEncoding(id.toUtf8()), size);
    }

private:
    Covers *m_covers;
};

} // namespace

QQuickAsyncImageProvider *Covers::provider() { return new Provider(this); }

// ── accent ─────────────────────────────────────────────────────────────
// The same pick as the terminal player's (internal/tui/accent.go): the
// dominant vivid hue, made readable on the background.

static double lum(const QColor &c)
{
    auto lin = [](double f) { return f <= 0.03928 ? f / 12.92 : std::pow((f + 0.055) / 1.055, 2.4); };
    return 0.2126 * lin(c.redF()) + 0.7152 * lin(c.greenF()) + 0.0722 * lin(c.blueF());
}

static double contrast(const QColor &a, const QColor &b)
{
    const double la = lum(a), lb = lum(b);
    return (std::max(la, lb) + 0.05) / (std::min(la, lb) + 0.05);
}

static QColor vivid(const QImage &src)
{
    const int bins = 24;
    double w[bins] = {}, sum[bins][3] = {};
    const QImage img = src.convertToFormat(QImage::Format_RGB32);
    const int step = std::max(1, std::max(img.width(), img.height()) / 48);
    int n = 0;
    for (int y = 0; y < img.height(); y += step) {
        for (int x = 0; x < img.width(); x += step) {
            const QColor c = QColor::fromRgb(img.pixel(x, y));
            n++;
            const double s = c.hsvSaturationF(), v = c.valueF();
            if (s < 0.25 || v < 0.2)
                continue;
            const int i = int(std::max(0.0f, c.hsvHueF()) * bins) % bins;
            const double wt = s * s * v;
            w[i] += wt;
            sum[i][0] += c.redF() * wt;
            sum[i][1] += c.greenF() * wt;
            sum[i][2] += c.blueF() * wt;
        }
    }
    int best = 0;
    double score = 0;
    for (int i = 0; i < bins; i++) {
        const double s = w[(i + bins - 1) % bins] / 2 + w[i] + w[(i + 1) % bins] / 2;
        if (s > score)
            best = i, score = s;
    }
    if (n == 0 || score < 0.03 * n)
        return {};
    double tw = 0, rgb[3] = {};
    for (int i : {(best + bins - 1) % bins, best, (best + 1) % bins}) {
        tw += w[i];
        for (int k = 0; k < 3; k++)
            rgb[k] += sum[i][k];
    }
    const QColor avg = QColor::fromRgbF(float(rgb[0] / tw), float(rgb[1] / tw), float(rgb[2] / tw));
    return QColor::fromHslF(std::max(0.0f, avg.hslHueF()), std::max(avg.hslSaturationF(), 0.55f), avg.lightnessF());
}

static QColor readable(QColor c, const QColor &bg)
{
    const float h = std::max(0.0f, c.hslHueF()), s = c.hslSaturationF();
    float l = c.lightnessF();
    const float dir = contrast(Qt::black, bg) > contrast(Qt::white, bg) ? -0.02f : 0.02f;
    for (int i = 0; i < 50 && contrast(c, bg) < 4.5 && l > 0 && l < 1; i++) {
        l = std::clamp(l + dir, 0.0f, 1.0f);
        c = QColor::fromHslF(h, s, l);
    }
    return c;
}

void Covers::accent(const QString &url, const QColor &bg)
{
    image(url, [this, url, bg](QImage img) {
        QThreadPool::globalInstance()->start([this, url, bg, img, self = QPointer<Covers>(this)] {
            QColor c = img.isNull() ? QColor() : vivid(img.scaled(96, 96));
            if (c.isValid())
                c = readable(c, bg);
            QMetaObject::invokeMethod(self, [this, url, c] { emit accentReady(url, c); });
        });
    });
}
