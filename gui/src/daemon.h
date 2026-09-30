// Daemon is the GUI's connection to the brumm daemon: newline-delimited
// JSON over its unix socket, the same protocol the terminal player speaks
// (internal/ipc). Requests carry an id and get one answer back; state and
// library changes arrive on their own.
#pragma once

#include <QHash>
#include <QJSEngine>
#include <QJSValue>
#include <QJsonObject>
#include <QLocalSocket>
#include <QObject>
#include <QTimer>

class Daemon : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    // The options all of brumm's players share, as the daemon keeps them.
    Q_PROPERTY(QVariantMap options READ options NOTIFY optionsChanged)
    Q_PROPERTY(bool omarchy READ omarchy NOTIFY optionsChanged) // the bar widget can be switched

public:
    explicit Daemon(QObject *parent = nullptr);
    ~Daemon() override;

    bool connected() const { return m_sock.state() == QLocalSocket::ConnectedState; }
    QVariantMap options() const { return m_options; }
    bool omarchy() const { return m_omarchy; }

    // listen asks for the spectrum and waveform (on) or stops them: only
    // the visualizer needs them, and they cost the daemon work.
    Q_INVOKABLE void listen(bool on);

    // setOption changes one shared option; done(error) when it is saved.
    Q_INVOKABLE void setOption(const QString &name, const QVariant &value, const QJSValue &done = QJSValue());

    // request sends req and calls done(reply) with the answer; a reply
    // with an error carries it in reply.error.
    Q_INVOKABLE void request(const QJSValue &req, const QJSValue &done = QJSValue());
    // send is request without an answer to wait for.
    Q_INVOKABLE void send(const QJSValue &req) { request(req); }

    // setEngine is the engine answers are made for.
    void setEngine(QJSEngine *e) { m_engine = e; }
    // forget drops the callbacks still waiting, before their engine goes.
    void forget()
    {
        m_pending.clear();
        m_engine = nullptr;
    }

signals:
    void connectedChanged();
    void optionsChanged();
    void state(const QVariantMap &state);
    void library();
    void failed(const QString &error);
    void sound(const QVector<int> &spectrum, const QVector<int> &wave);

private:
    void connectNow();
    void readLines();
    void subscribe();
    void write(QJsonObject req, const QJSValue &done);

    QLocalSocket m_sock;
    QTimer m_retry;
    QByteArray m_buf;
    int m_next = 0;
    QHash<int, QJSValue> m_pending;
    QJSEngine *m_engine = nullptr;
    QVariantMap m_options;
    bool m_omarchy = false;
    bool m_listening = false;
};
