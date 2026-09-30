QT += core gui qml quick quickcontrols2 network dbus
CONFIG += c++17 release
TARGET = brumm-gui
TEMPLATE = app

HEADERS += $$files(src/*.h)
SOURCES += $$files(src/*.cpp)

# Everything under qml/ and shaders/ is built in, gathered when qmake runs:
# the QML as :/qml/..., the shaders compiled by Qt's qsb (qt6-shadertools)
# as :/shaders/NAME.qsb. A new file needs no listing, only a new qmake.
QSB = $$[QT_HOST_BINS]/qsb
!exists($$QSB): QSB = $$[QT_HOST_LIBEXECS]/qsb
!exists($$QSB): error("the window's shaders need qsb: install qt6-shadertools")
QRC = "<RCC><qresource prefix=\"/\">"
for(f, $$list($$files($$PWD/qml/*.qml, true))) {
    QRC += "<file alias=\"qml/$$relative_path($$f, $$PWD/qml)\">$$f</file>"
}
QRC += "</qresource><qresource prefix=\"/shaders\">"
for(s, $$list($$files($$PWD/shaders/*.frag) $$files($$PWD/shaders/*.vert))) {
    name = $$basename(s)
    QSB_CMD = $$QSB --glsl $$shell_quote("100 es,120,150") --hlsl 50 --msl 12 -o $$shell_quote($$OUT_PWD/$${name}.qsb) $$shell_quote($$s)
    !system($$QSB_CMD): error("qsb failed on $$name")
    QRC += "<file>$${name}.qsb</file>"
}
QRC += "</qresource></RCC>"
write_file($$OUT_PWD/brumm-gui.qrc, QRC)
RESOURCES += $$OUT_PWD/brumm-gui.qrc
