QT += core gui qml quick quickcontrols2 network dbus
CONFIG += c++17 release
TARGET = brumm-gui
TEMPLATE = app

# dev/build-gui builds the Rust calculations first and supplies their path.
isEmpty(BRUMM_SHARED_LIB): BRUMM_SHARED_LIB = $$PWD/../target/release/libbrumm_shared.a
!exists($$BRUMM_SHARED_LIB): error("Build crates/brumm-shared first (dev/build-gui)")
LIBS += $$quote($$BRUMM_SHARED_LIB) -ldl -lpthread -lm
PRE_TARGETDEPS += $$quote($$BRUMM_SHARED_LIB)

HEADERS += $$files(src/*.h)
SOURCES += $$files(src/*.cpp)

# Everything under qml/ and shaders/ is built in, gathered when qmake runs:
# the QML and its pictures (the visualizer's previews) as :/qml/..., the
# shaders compiled by Qt's qsb (qt6-shadertools) as :/shaders/NAME.qsb.
# A new file needs no listing, only a new qmake.
QSB = $$[QT_HOST_BINS]/qsb
!exists($$QSB): QSB = $$[QT_HOST_LIBEXECS]/qsb
!exists($$QSB): error("the window's shaders need qsb: install qt6-shadertools")
QRC = "<RCC><qresource prefix=\"/\">"
for(f, $$list($$files($$PWD/qml/*.qml, true) $$files($$PWD/qml/*.jpg, true))) {
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
