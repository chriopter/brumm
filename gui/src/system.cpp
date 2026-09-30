#include "system.h"

#include <QClipboard>
#include <QGuiApplication>

void System::copy(const QString &text) { QGuiApplication::clipboard()->setText(text); }

QVariant System::setting(const QString &key, const QVariant &fallback) const { return m_settings.value(key, fallback); }

void System::setSetting(const QString &key, const QVariant &value) { m_settings.setValue(key, value); }
