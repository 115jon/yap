cmake_minimum_required(VERSION 3.24)
# The Qt Quick mobile prototype (desktop-native/mobile/README.md): the
# desktop's cxx-qt pattern (CMakeLists.txt one folder up) with Qt for
# Android. Configure with Qt's Android toolchain (build.sh does):
#   qt.toolchain.cmake, QT_HOST_PATH (a desktop Qt of the same version),
#   ANDROID_SDK_ROOT, ANDROID_NDK_ROOT.
project(YapQtProto VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# Only what the shared QML and the shell use: no Multimedia, WebView,
# Widgets, Svg widgets, Quick 3D or the Controls styles beyond Basic.
find_package(Qt6 6.11 REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2 Svg)
find_package(Qt6 REQUIRED COMPONENTS QuickPrivate)

include(FetchContent)
FetchContent_Declare(
  CxxQt
  GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git
  # The desktop's pin (main as of 2026-11-04).
  GIT_TAG 64e63f560737b3b7f85ef8267c5918e4b8176d5a)
FetchContent_MakeAvailable(CxxQt)

# The target Qt's qmake (its wrapper runs the host qmake on the target's
# qt.conf): Qt::qmake is the host tool in a cross build, and cxx-qt-build
# would read the host's headers through it.
set(YAP_TARGET_QMAKE "${QT6_INSTALL_PREFIX}/${QT6_INSTALL_BINS}/qmake")
if(CMAKE_HOST_WIN32)
  string(APPEND YAP_TARGET_QMAKE ".bat")
endif()
cxx_qt_import_crate(
  MANIFEST_PATH rust/Cargo.toml
  CRATES yap_mobile_qt
  LOCKED
  QMAKE "${YAP_TARGET_QMAKE}"
  QT_MODULES Qt::Core Qt::Gui Qt::Qml Qt::Quick Qt::QuickControls2)

cxx_qt_import_qml_module(yap_mobile_qml
  URI "com.yap.mobile"
  SOURCE_CRATE yap_mobile_qt)
# The view layer shared with the desktop, built by the yap-ui crate.
cxx_qt_import_qml_module(yap_ui_qml
  URI "Yap.Ui"
  SOURCE_CRATE yap_mobile_qt)

set(DESKTOP ${CMAKE_CURRENT_SOURCE_DIR}/../..)
# The C++ behind Yap.Ui (AnimatedPicture, the icon and dither providers),
# shared with the desktop; it compiles in this target.
include(${DESKTOP}/rust/crates/yap-ui/cpp/yap_ui.cmake)

qt_add_executable(yapqt
  cpp/main.cpp
  cpp/native.cpp cpp/native.h)
target_include_directories(yapqt PRIVATE cpp)
target_link_libraries(yapqt PRIVATE
  yap_mobile_qml
  yap_ui_qml
  yap_ui_cpp
  Qt::Gui Qt::Network Qt::Quick Qt::QuickControls2 Qt::Svg Qt::QuickPrivate)

set_target_properties(yapqt PROPERTIES
  QT_ANDROID_PACKAGE_NAME dev.yap.qtproto
  QT_ANDROID_APP_NAME "Yap (Qt)"
  QT_ANDROID_PACKAGE_SOURCE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/android
  QT_ANDROID_MIN_SDK_VERSION 28
  QT_ANDROID_TARGET_SDK_VERSION 36
  QT_ANDROID_VERSION_CODE 1
  QT_ANDROID_VERSION_NAME 0.1.0
  # Where androiddeployqt's import scan looks for the QML modules to
  # bundle: this app's and the shared Yap.Ui's, which lives in yap-ui.
  QT_QML_ROOT_PATH "${CMAKE_CURRENT_SOURCE_DIR};${DESKTOP}/rust/crates/yap-ui/qml")

# The desktop's brand fonts and Phosphor icons (desktop-native/assets),
# embedded as :/fonts and :/icons as on the desktop.
file(GLOB YAP_FONT_FILES RELATIVE ${DESKTOP}/assets CONFIGURE_DEPENDS ${DESKTOP}/assets/fonts/*.ttf)
file(STRINGS ${DESKTOP}/assets/icons/files.txt YAP_ICON_NAMES)
list(TRANSFORM YAP_ICON_NAMES PREPEND icons/ OUTPUT_VARIABLE YAP_ICON_FILES)
set(YAP_ASSET_FILES ${YAP_FONT_FILES} ${YAP_ICON_FILES})
list(TRANSFORM YAP_ASSET_FILES PREPEND ${DESKTOP}/assets/ OUTPUT_VARIABLE YAP_ASSET_PATHS)
qt_add_resources(yapqt yap_assets
  PREFIX /
  BASE ${DESKTOP}/assets
  FILES ${YAP_ASSET_PATHS})

# Only the plugins the app uses (androiddeployqt otherwise ships every one
# of each module): GIF, JPEG, WebP and SVG pictures; no QML debugger or
# profiler services, no icon engines or network-information backends.
qt_import_plugins(yapqt
  INCLUDE_BY_TYPE imageformats Qt::QGifPlugin Qt::QJpegPlugin Qt::QWebpPlugin Qt::QSvgPlugin
  EXCLUDE_BY_TYPE qmltooling iconengines networkinformation)
