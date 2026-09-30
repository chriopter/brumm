// Theme is the Omarchy theme brumm's window wears: the colors of
// ~/.local/state/omarchy/current/theme/colors.toml, read again whenever
// the theme changes, and the desktop's text size.
#pragma once

#include <QColor>
#include <QDBusVariant>
#include <QFileSystemWatcher>
#include <QObject>
#include <QVariantMap>

class Theme : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap colors READ colors NOTIFY changed)
    Q_PROPERTY(bool dark READ dark NOTIFY changed)
    Q_PROPERTY(double textScale READ textScale NOTIFY textScaleChanged)

public:
    explicit Theme(QObject *parent = nullptr);

    QVariantMap colors() const { return m_colors; }
    bool dark() const { return m_dark; }
    double textScale() const { return m_textScale; }

signals:
    void changed();
    void textScaleChanged();

private slots:
    void portalSettingChanged(const QString &ns, const QString &key, const QDBusVariant &value);

private:
    void load();
    void watch();
    void readTextScale();

    QVariantMap m_colors;
    bool m_dark = true;
    double m_textScale = 1.0;
    QFileSystemWatcher m_watcher;
};
