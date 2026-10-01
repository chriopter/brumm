// brumm-gui is brumm as a window: the same player as the terminal one,
// talking to the same daemon, in the Omarchy theme.
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QFontDatabase>
#include <QFontInfo>
#include <QGuiApplication>
#include <QLockFile>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QStandardPaths>
#include <QThread>
#include <QThreadPool>
#include <QMouseEvent>
#include <QTimer>
#include <atomic>

#include "covers.h"
#include "daemon.h"
#include "system.h"
#include "theme.h"
#include "vizaudio.h"

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    app.setApplicationName("brumm");
    app.setOrganizationName("brumm");
    app.setDesktopFileName("brumm"); // the Wayland app id, for Hyprland's rules

    // One window: a second start brings the first to the front.
    QString runtime = qEnvironmentVariable("XDG_RUNTIME_DIR");
    if (runtime.isEmpty()) { // not the shared /tmp, where another user could hold it
        runtime = QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation) + "/brumm";
        QDir().mkpath(runtime);
    }
    QLockFile lock(runtime + "/brumm-gui.lock");
    lock.setStaleLockTime(0);
    if (!lock.tryLock(100)) {
        // Hyprland's Lua config, else its older syntax.
        if (QProcess::execute("hyprctl", {"dispatch", "hl.dsp.focus({ window = \"class:^brumm$\" })"}) != 0)
            QProcess::execute("hyprctl", {"dispatch", "focuswindow", "class:^brumm$"});
        return 0;
    }

    QQuickStyle::setStyle("Basic");
    // Text in the desktop's sans-serif; icons and figures in the monospace
    // font Omarchy sets (omarchy font set), which carries the Nerd icons.
    // Adwaita Sans where GNOME's fonts are, as on Omarchy; else what is.
    QString sans = QFontInfo(QFont("sans-serif")).family();
    for (const char *f : {"Adwaita Sans", "Inter", "Cantarell"})
        if (QFontDatabase::hasFamily(f)) {
            sans = f;
            break;
        }
    const QString mono = QFontInfo(QFont("monospace")).family();

    Theme theme;
    Daemon daemon;
    Covers covers;
    System system;
    VizAudio viz;
    QObject::connect(&daemon, &Daemon::sound, &viz, &VizAudio::feed);

    qmlRegisterType<VizTexture>("Brumm", 1, 0, "VizTexture");
    QQmlApplicationEngine engine;
    daemon.setEngine(&engine);

    // An update replaces this program and restarts the daemon: once it is
    // back, the window starts again as the new one.
    // (The path is taken now: once replaced, /proc says "(deleted)".)
    const QString self = QCoreApplication::applicationFilePath();
    const QDateTime started = QDateTime::currentDateTime();
    QObject::connect(&daemon, &Daemon::connectedChanged, &app, [&, self] {
        if (!daemon.connected() || QFileInfo(self).lastModified() <= started)
            return;
        lock.unlock();
        QProcess::startDetached(self, {});
        QCoreApplication::quit();
    });
    engine.addImageProvider("cover", covers.provider());
    auto *ctx = engine.rootContext();
    ctx->setContextProperty("theme", &theme);
    ctx->setContextProperty("daemon", &daemon);
    ctx->setContextProperty("covers", &covers);
    ctx->setContextProperty("sys", &system);
    ctx->setContextProperty("vizAudio", &viz);
    ctx->setContextProperty("monoFont", mono);
    ctx->setContextProperty("sansFont", sans);
    // For tests: keys to press once the window is up, as the terminal
    // player names them, separated by spaces.
    ctx->setContextProperty("testKeys", qEnvironmentVariable("BRUMM_GUI_KEYS"));
    const QStringList size = qEnvironmentVariable("BRUMM_GUI_SIZE").split('x');
    ctx->setContextProperty("testSize", size.size() == 2 ? QSize(size[0].toInt(), size[1].toInt()) : QSize(0, 0));
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, [](const QList<QQmlError> &ws) {
        for (const auto &w : ws)
            qWarning().noquote() << w.toString();
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); },
                     Qt::QueuedConnection);
    engine.load(QUrl("qrc:/qml/Main.qml"));

    // Omarchy lays its window transparency over every window; the player's
    // light and covers want to be seen whole, so this one is opaque.
    auto opaque = [] {
        if (QStandardPaths::findExecutable("hyprctl").isEmpty())
            return;
        QProcess::startDetached("sh", {"-c", "hyprctl dispatch 'hl.dsp.window.set_prop({ window = \"class:^brumm$\", prop = \"opaque\", value = \"1\" })'"
                                             " >/dev/null 2>&1 || hyprctl dispatch setprop 'class:^brumm$' opaque 1 >/dev/null 2>&1"});
    };
    QTimer::singleShot(300, &app, opaque);
    QTimer::singleShot(1500, &app, opaque); // once more, should the window have come late

    // For tests: BRUMM_GUI_CLICKS="x,y x,y …" clicks there, one a quarter
    // second, starting a second after the window opens.
    if (const QStringList clicks = qEnvironmentVariable("BRUMM_GUI_CLICKS").split(' ', Qt::SkipEmptyParts); !clicks.isEmpty()) {
        for (int i = 0; i < clicks.size(); i++) {
            const QStringList xy = clicks[i].split(',');
            const QPointF at(xy.value(0).toDouble(), xy.value(1).toDouble());
            QTimer::singleShot(1000 + 250 * i, &app, [&engine, at] {
                auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
                if (!w)
                    return;
                for (auto type : {QEvent::MouseButtonPress, QEvent::MouseButtonRelease}) {
                    QMouseEvent e(type, at, w->mapToGlobal(at), Qt::LeftButton,
                                  type == QEvent::MouseButtonPress ? Qt::LeftButton : Qt::NoButton, Qt::NoModifier);
                    QCoreApplication::sendEvent(w, &e);
                }
            });
        }
    }

    // For tests and screenshots: BRUMM_GUI_SHOT=file.png saves the window
    // a moment after it opens, then quits.
    if (const QString shot = qEnvironmentVariable("BRUMM_GUI_SHOT"); !shot.isEmpty()) {
        QTimer::singleShot(qEnvironmentVariableIntValue("BRUMM_GUI_SHOT_MS") ?: 1500, &app, [&engine, shot] {
            if (auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0)))
                w->grabWindow().save(shot);
            QCoreApplication::quit();
        });
    }
    // For measuring (bin/bench): BRUMM_GUI_PACE=HZ paces the frames as a
    // display of HZ would (off screen, nothing waits for one), and
    // BRUMM_GUI_FRAMES=FILE keeps the count of frames drawn in FILE, with
    // the milliseconds it was taken at, written once a second.
    if (auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0))) {
        if (const int hz = qEnvironmentVariableIntValue("BRUMM_GUI_PACE"); hz > 0) {
            const qint64 period = 1000000000LL / hz;
            QObject::connect(w, &QQuickWindow::afterFrameEnd, w, [period] {
                static QElapsedTimer clock;
                static qint64 next = 0;
                if (!clock.isValid())
                    clock.start();
                const qint64 now = clock.nsecsElapsed();
                next = std::max(next + period, now);
                QThread::usleep((next - now) / 1000);
            }, Qt::DirectConnection);
        }
        if (const QString file = qEnvironmentVariable("BRUMM_GUI_FRAMES"); !file.isEmpty()) {
            static std::atomic<qint64> frames = 0;
            QObject::connect(w, &QQuickWindow::frameSwapped, w, [] { frames++; }, Qt::DirectConnection);
            auto *t = new QTimer(&app);
            QObject::connect(t, &QTimer::timeout, [file] {
                QFile f(file);
                if (f.open(QIODevice::WriteOnly | QIODevice::Truncate))
                    f.write(QByteArray::number(frames.load()) + " " + QByteArray::number(QDateTime::currentMSecsSinceEpoch()) + "\n");
            });
            t->start(1000);
        }
    }
    const int code = app.exec();
    daemon.forget();
    QThreadPool::globalInstance()->waitForDone(2000); // covers still decoding
    return code;
}
