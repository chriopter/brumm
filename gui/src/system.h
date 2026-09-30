// System is what the window keeps for itself: the clipboard, and its
// own small settings (where the divider sits). Everything brumm shares
// between its players goes through the daemon.
#pragma once

#include <QObject>
#include <QSettings>

class System : public QObject {
    Q_OBJECT

public:
    using QObject::QObject;

    Q_INVOKABLE void copy(const QString &text);
    Q_INVOKABLE QVariant setting(const QString &key, const QVariant &fallback = {}) const;
    Q_INVOKABLE void setSetting(const QString &key, const QVariant &value);

private:
    QSettings m_settings;
};
