#include "covers.h"
#include "brumm_shared.h"

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

// The covers the terminal player keeps (crates/brumm/src/art): the same folder,
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
// The same pick as the terminal player's (crates/brumm-shared/src/accent.rs): the
// dominant vivid hue, made readable on the background.

void Covers::accent(const QString &url, const QColor &bg)
{
    image(url, [this, url, bg](QImage img) {
        QThreadPool::globalInstance()->start([this, url, bg, img, self = QPointer<Covers>(this)] {
            QColor c;
            if (!img.isNull()) {
                const QImage rgba = img.scaled(96, 96).convertToFormat(QImage::Format_RGBA8888);
                std::uint32_t rgb = 0;
                if (brumm_accent(rgba.constBits(), rgba.width(), rgba.height(), rgba.bytesPerLine(),
                                 bg.rgb() & 0xffffff, &rgb))
                    c = QColor::fromRgb(rgb);
            }
            QMetaObject::invokeMethod(self, [this, url, c] { emit accentReady(url, c); });
        });
    });
}
