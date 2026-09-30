// Covers loads album art for the window: image://cover/<address> reads
// the cover cache the terminal player shares (~/.cache/brumm/covers), and
// fetches and keeps what it lacks. It also picks a cover's accent color.
#pragma once

#include <QColor>
#include <QHash>
#include <QNetworkAccessManager>
#include <QQuickAsyncImageProvider>

class Covers : public QObject {
    Q_OBJECT

public:
    explicit Covers(QObject *parent = nullptr);

    // accent picks the cover's most vivid color, readable on bg; the
    // answer comes as accentReady. A gray cover answers an invalid color.
    Q_INVOKABLE void accent(const QString &url, const QColor &bg);

    // image calls done with the cover at url, from the cache or the net.
    void image(const QString &url, std::function<void(QImage)> done);

    QQuickAsyncImageProvider *provider();

signals:
    void accentReady(const QString &url, const QColor &color);

private:
    QNetworkAccessManager m_net;
    QHash<QString, QList<std::function<void(QImage)>>> m_waiting;
};
