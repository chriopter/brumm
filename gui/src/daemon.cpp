#include "daemon.h"

#include <QJSEngine>
#include <QJsonArray>
#include <QJsonDocument>
#include <QCoreApplication>
#include <QFileInfo>
#include <QProcess>
#include <QStandardPaths>

// socketPath is config.Socket(): the runtime dir, else the cache.
static QString socketPath()
{
    QString dir = qEnvironmentVariable("XDG_RUNTIME_DIR");
    if (dir.isEmpty())
        dir = QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation) + "/brumm";
    return dir + "/brumm.sock";
}

// brummPath is the brumm next to this program, else the one on the path.
static QString brummPath()
{
    const QString here = QCoreApplication::applicationDirPath() + "/brumm";
    return QFileInfo(here).isExecutable() ? here : QStringLiteral("brumm");
}

Daemon::Daemon(QObject *parent) : QObject(parent)
{
    m_retry.setInterval(1000);
    m_retry.setSingleShot(true);
    connect(&m_retry, &QTimer::timeout, this, &Daemon::connectNow);
    connect(&m_sock, &QLocalSocket::connected, this, [this] {
        emit connectedChanged();
        subscribe();
    });
    connect(&m_sock, &QLocalSocket::disconnected, this, [this] {
        // Unanswered requests fail; the daemon restarts (an update) and
        // is connected to again.
        const auto pending = m_pending;
        m_pending.clear();
        for (auto done : pending)
            if (done.isCallable())
                done.call(QJSValueList{QJSValue(QStringLiteral("daemon disconnected"))});
        m_buf.clear();
        emit connectedChanged();
        m_retry.start();
    });
    connect(&m_sock, &QLocalSocket::errorOccurred, this, [this](QLocalSocket::LocalSocketError) {
        if (m_sock.state() != QLocalSocket::ConnectedState && !m_retry.isActive()) {
            // Not there: ask brumm to start it, as the terminal player does.
            // The systemd unit if there is one, else brumm daemon on its own.
            static bool started = false;
            if (!started) {
                started = true;
                auto *p = new QProcess(this);
                connect(p, &QProcess::finished, this, [p](int code, QProcess::ExitStatus status) {
                    p->deleteLater();
                    if (status != QProcess::NormalExit || code != 0)
                        QProcess::startDetached(brummPath(), {"daemon"});
                });
                connect(p, &QProcess::errorOccurred, this, [p](QProcess::ProcessError e) {
                    if (e != QProcess::FailedToStart)
                        return;
                    p->deleteLater();
                    QProcess::startDetached(brummPath(), {"daemon"});
                });
                p->start(QStringLiteral("systemctl"), {"--user", "start", "brumm.service"});
            }
            m_retry.start();
        }
    });
    connect(&m_sock, &QLocalSocket::readyRead, this, &Daemon::readLines);
    connectNow();
}

// The socket goes last of the members, and closing says "disconnected":
// nothing may answer that once the rest is gone.
Daemon::~Daemon()
{
    m_sock.disconnect(this);
    m_retry.stop();
    m_pending.clear();
    m_sock.abort();
}

void Daemon::connectNow()
{
    if (m_sock.state() == QLocalSocket::UnconnectedState)
        m_sock.connectToServer(socketPath());
}

void Daemon::subscribe()
{
    // State, and the sound only while the visualizer shows.
    if (m_listening)
        write(QJsonObject{{"cmd", "subscribe"}, {"bands", 64}, {"wave", 256}, {"fps", 60}}, QJSValue());
    else
        write(QJsonObject{{"cmd", "subscribe"}}, QJSValue());
    write(QJsonObject{{"cmd", "options"}}, QJSValue());
}

void Daemon::listen(bool on)
{
    if (on == m_listening)
        return;
    m_listening = on;
    if (connected())
        write(on ? QJsonObject{{"cmd", "subscribe"}, {"bands", 64}, {"wave", 256}, {"fps", 60}} : QJsonObject{{"cmd", "subscribe"}},
              QJSValue());
}

void Daemon::setOption(const QString &name, const QVariant &value, const QJSValue &done)
{
    write(QJsonObject{{"cmd", "options"}, {"options", QJsonObject{{name, QJsonValue::fromVariant(value)}}}}, done);
}

void Daemon::request(const QJSValue &req, const QJSValue &done)
{
    write(QJsonObject::fromVariantMap(req.toVariant().toMap()), done);
}

void Daemon::write(QJsonObject req, const QJSValue &done)
{
    if (!connected()) {
        if (done.isCallable())
            done.call(QJSValueList{QJSValue(QStringLiteral("not connected to the brumm daemon"))});
        return;
    }
    const int id = ++m_next;
    req.insert("id", id);
    m_pending.insert(id, done);
    m_sock.write(QJsonDocument(req).toJson(QJsonDocument::Compact) + '\n');
}

void Daemon::readLines()
{
    m_buf += m_sock.readAll();
    qsizetype nl;
    while ((nl = m_buf.indexOf('\n')) >= 0) {
        const QByteArray line = m_buf.left(nl);
        m_buf.remove(0, nl + 1);
        const QJsonObject msg = QJsonDocument::fromJson(line).object();
        if (msg.isEmpty())
            continue;
        if (msg.contains("options")) {
            const QVariantMap o = msg.value("options").toObject().toVariantMap();
            const bool omarchy = msg.contains("omarchy") ? msg.value("omarchy").toBool() : m_omarchy;
            if (o != m_options || omarchy != m_omarchy) {
                m_options = o;
                m_omarchy = omarchy;
                emit optionsChanged();
            }
        }
        const int id = msg.value("id").toInt();
        if (id != 0) {
            QJSValue done = m_pending.take(id);
            if (done.isCallable() && m_engine) {
                const QString err = msg.value("error").toString();
                done.call(QJSValueList{err.isEmpty() ? QJSValue(QJSValue::NullValue) : QJSValue(err),
                                       m_engine->toScriptValue(msg.toVariantMap())});
            }
            if (msg.contains("state"))
                emit state(msg.value("state").toObject().toVariantMap());
            continue;
        }
        if (msg.contains("state"))
            emit state(msg.value("state").toObject().toVariantMap());
        if (msg.contains("spectrum") || msg.contains("wave")) {
            QVector<int> spec, wave;
            for (const auto v : msg.value("spectrum").toArray())
                spec << v.toInt();
            for (const auto v : msg.value("wave").toArray())
                wave << v.toInt();
            emit sound(spec, wave);
        }
        if (msg.value("library").toBool())
            emit library();
    }
}
