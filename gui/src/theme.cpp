#include "theme.h"

#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusReply>
#include <QDBusVariant>
#include <QDir>
#include <QFile>
#include <QTextStream>
#include <QTimer>

static QString currentDir() { return QDir::homePath() + "/.local/state/omarchy/current"; }
static QString colorsPath() { return currentDir() + "/theme/colors.toml"; }

// The colors a theme without its colors.toml falls back to: Omarchy's
// own dark defaults.
static const QVariantMap fallback = {
    {"background", "#1a1b26"}, {"dark_background", "#13141c"}, {"darker_background", "#0e0e14"},
    {"lighter_background", "#24283b"}, {"foreground", "#a9b1d6"}, {"dark_foreground", "#565f89"},
    {"light_foreground", "#b4bee6"}, {"bright_foreground", "#c0caf5"}, {"accent", "#7aa2f7"},
    {"selection", "#292e42"}, {"muted", "#414868"}, {"red", "#f7768e"}, {"yellow", "#e0af68"},
    {"green", "#9ece6a"}, {"cyan", "#449dab"}, {"blue", "#7aa2f7"}, {"magenta", "#ad8ee6"},
};

Theme::Theme(QObject *parent) : QObject(parent)
{
    load();
    watch();
    // A theme switch replaces files and directories: read again once it
    // settles, and watch what is there now.
    auto again = [this] {
        QTimer::singleShot(150, this, [this] {
            load();
            watch();
        });
    };
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, this, again);
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, this, again);

    readTextScale();
    QDBusConnection::sessionBus().connect("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
                                          "org.freedesktop.portal.Settings", "SettingChanged", this,
                                          SLOT(portalSettingChanged(QString, QString, QDBusVariant)));
}

void Theme::load()
{
    QVariantMap c = fallback;
    QString mode;
    QFile f(colorsPath());
    if (f.open(QIODevice::ReadOnly | QIODevice::Text)) {
        QTextStream in(&f);
        while (!in.atEnd()) {
            const QString line = in.readLine().trimmed();
            const int eq = line.indexOf('=');
            if (line.isEmpty() || line.startsWith('#') || eq < 0)
                continue;
            const QString key = line.left(eq).trimmed();
            QString value = line.mid(eq + 1).trimmed();
            if (value.size() >= 2 && (value.front() == '"' || value.front() == '\'') && value.back() == value.front())
                value = value.mid(1, value.size() - 2);
            if (key == "mode")
                mode = value;
            else if (QColor::isValidColorName(value))
                c.insert(key, value);
        }
    }
    const QColor bg(c.value("background").toString());
    const bool dark = mode == "light" ? false : mode == "dark" ? true : bg.lightnessF() < 0.5;
    if (c != m_colors || dark != m_dark) {
        m_colors = c;
        m_dark = dark;
        emit changed();
    }
}

void Theme::watch()
{
    const QStringList was = m_watcher.files() + m_watcher.directories();
    if (!was.isEmpty())
        m_watcher.removePaths(was);
    for (const QString &p : {currentDir(), currentDir() + "/theme", colorsPath()})
        if (QFile::exists(p))
            m_watcher.addPath(p);
}

// readTextScale asks the portal for the desktop's text size, which
// `omarchy display text size` sets.
void Theme::readTextScale()
{
    auto msg = QDBusMessage::createMethodCall("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
                                              "org.freedesktop.portal.Settings", "ReadOne");
    msg << QStringLiteral("org.gnome.desktop.interface") << QStringLiteral("text-scaling-factor");
    QDBusReply<QDBusVariant> reply = QDBusConnection::sessionBus().call(msg, QDBus::Block, 150);
    if (reply.isValid())
        portalSettingChanged("org.gnome.desktop.interface", "text-scaling-factor", reply.value());
}

void Theme::portalSettingChanged(const QString &ns, const QString &key, const QDBusVariant &value)
{
    if (ns != "org.gnome.desktop.interface" || key != "text-scaling-factor")
        return;
    QVariant v = value.variant();
    if (v.canConvert<QDBusVariant>())
        v = v.value<QDBusVariant>().variant();
    bool ok = false;
    const double s = v.toDouble(&ok);
    if (ok && s >= 0.5 && s <= 3.0 && s != m_textScale) {
        m_textScale = s;
        emit textScaleChanged();
    }
}
