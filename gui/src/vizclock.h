#pragma once
#include <QElapsedTimer>
#include <QObject>
#include <QTimer>
#include <atomic>
class QScreen;

// One clock for animation and feedback, with a cap on extra scenegraph frames.
class VizClock : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool running READ running WRITE setRunning NOTIFY runningChanged)
    Q_PROPERTY(int fps READ fps WRITE setFps NOTIFY fpsChanged)
    Q_PROPERTY(bool limitFrames READ limitFrames WRITE setLimitFrames NOTIFY limitFramesChanged)
    Q_PROPERTY(double frameTime READ frameTime NOTIFY triggered)
    Q_PROPERTY(int refreshRate READ refreshRate NOTIFY refreshRateChanged)
public:
    explicit VizClock(QObject *parent = nullptr);
    bool running() const { return m_running; }
    int fps() const { return m_fps; }
    bool limitFrames() const { return m_limitFrames; }
    double frameTime() const { return m_frameTime; }
    int refreshRate() const { return m_refreshRate; }
    void setRunning(bool running);
    void setFps(int fps);
    void setLimitFrames(bool limit);
    void paceFrame(); // render thread, after a frame has finished
    void setScreen(QScreen *screen);
signals:
    void triggered();
    void runningChanged();
    void fpsChanged();
    void limitFramesChanged();
    void refreshRateChanged();
private:
    void reset();
    void schedule();
    QTimer m_timer;
    QElapsedTimer m_time;
    bool m_running = false;
    int m_fps = 30;
    bool m_limitFrames = true;
    qint64 m_last = 0, m_next = 0;
    double m_frameTime = 1.0 / 30;
    std::atomic<int> m_drawRate{0};
    QElapsedTimer m_drawTime;
    qint64 m_drawNext = 0;
    int m_previousRate = 0;
    int m_refreshRate = 60;
    QMetaObject::Connection m_screenChanged;
};
