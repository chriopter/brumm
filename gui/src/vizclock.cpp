#include "vizclock.h"
#include <algorithm>
#include <QThread>
#include <QScreen>
#include <cmath>

VizClock::VizClock(QObject *parent) : QObject(parent)
{
    m_timer.setSingleShot(true);
    m_timer.setTimerType(Qt::PreciseTimer);
    connect(&m_timer, &QTimer::timeout, this, [this] {
        const qint64 now = m_time.nsecsElapsed();
        m_frameTime = std::min(0.1, (now - m_last) / 1e9);
        m_last = now;
        m_next = std::max<qint64>(m_next + 1000000000LL / m_fps, now);
        emit triggered();
        if (m_running) schedule();
    });
}
void VizClock::setRunning(bool running)
{
    if (m_running == running) return;
    m_running = running;
    m_drawRate.store(running && m_limitFrames ? m_fps : 0);
    if (running) reset(); else m_timer.stop();
    emit runningChanged();
}
void VizClock::setFps(int fps)
{
    fps = std::clamp(fps, 1, 240);
    if (m_fps == fps) return;
    m_fps = fps;
    m_drawRate.store(m_running && m_limitFrames ? fps : 0);
    if (m_running) reset();
    emit fpsChanged();
}
void VizClock::setLimitFrames(bool limit)
{
    if (m_limitFrames == limit) return;
    m_limitFrames = limit;
    m_drawRate.store(m_running && limit ? m_fps : 0);
    emit limitFramesChanged();
}
void VizClock::reset()
{
    m_time.start();
    m_last = 0;
    m_next = 1000000000LL / m_fps;
    schedule();
}
void VizClock::schedule()
{
    const qint64 remaining = m_next - m_time.nsecsElapsed();
    m_timer.start(int(std::max<qint64>(1, (remaining + 999999) / 1000000)));
}

void VizClock::paceFrame()
{
    const int rate = m_drawRate.load();
    if (!rate) { m_previousRate = 0; return; }
    if (rate != m_previousRate) {
        m_drawTime.start();
        m_drawNext = 0;
        m_previousRate = rate;
    }
    const qint64 now = m_drawTime.nsecsElapsed();
    m_drawNext = std::max<qint64>(m_drawNext + 1000000000LL / rate, now);
    const qint64 remaining = m_drawNext - now;
    if (remaining > 0) QThread::usleep(static_cast<unsigned long>(remaining / 1000));
}

void VizClock::setScreen(QScreen *screen)
{
    disconnect(m_screenChanged);
    const auto update = [this](qreal rate) {
        const int next = std::isfinite(rate) && rate > 0 ? qRound(rate) : 60;
        if (next == m_refreshRate) return;
        m_refreshRate = next;
        emit refreshRateChanged();
    };
    update(screen ? screen->refreshRate() : 60);
    if (screen)
        m_screenChanged = connect(screen, &QScreen::refreshRateChanged, this, update);
}
