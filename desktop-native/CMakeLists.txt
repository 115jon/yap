cmake_minimum_required(VERSION 3.24)
project(YapNative VERSION 0.1.0 LANGUAGES CXX)

# The static Qt build uses the static MSVC runtime (/MT); the Rust side
# matches it through rust/.cargo/config.toml (+crt-static).
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
# Sources and generated code are UTF-8; MSVC otherwise assumes the ANSI code page.
add_compile_options($<$<CXX_COMPILER_ID:MSVC>:/utf-8>)

find_package(Qt6 REQUIRED COMPONENTS
  Core Gui Multimedia Network Qml Quick QuickControls2 Svg QmlImportScanner)

include(FetchContent)
FetchContent_Declare(
  CxxQt
  GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git
  # main as of 2026-11-04 (no tag for cxx-qt 0.10 yet)
  GIT_TAG 64e63f560737b3b7f85ef8267c5918e4b8176d5a)
FetchContent_MakeAvailable(CxxQt)

cxx_qt_import_crate(
  MANIFEST_PATH rust/Cargo.toml
  CRATES yap_native
  LOCKED
  QT_MODULES Qt::Core Qt::Gui Qt::Qml Qt::Quick Qt::QuickControls2)

cxx_qt_import_qml_module(yap_native_qml
  URI "com.yap.native"
  SOURCE_CRATE yap_native)

qt_add_executable(YapNative WIN32
  cpp/main.cpp
  cpp/app.rc
  cpp/camera.cpp cpp/camera.h
  cpp/desktopshell.cpp cpp/desktopshell.h
  cpp/ditherprovider.cpp cpp/ditherprovider.h
  cpp/iconprovider.cpp cpp/iconprovider.h
  cpp/logfile.cpp cpp/logfile.h
  cpp/mediashell.cpp cpp/mediashell.h
  cpp/toast_win.cpp cpp/toast_win.h
  cpp/videohub.cpp cpp/videohub.h
  cpp/windowchrome.cpp cpp/windowchrome.h)
target_link_libraries(YapNative PRIVATE
  yap_native_qml
  Qt::Gui Qt::Multimedia Qt::Network Qt::Quick Qt::QuickControls2 Qt::Svg)

# Brand fonts, Phosphor icons and brand SVGs (assets/, built by tools/),
# embedded as :/fonts, :/icons and :/svg.
file(GLOB YAP_FONT_FILES RELATIVE ${CMAKE_CURRENT_SOURCE_DIR}/assets
  CONFIGURE_DEPENDS assets/fonts/*.ttf)
file(GLOB YAP_SVG_FILES RELATIVE ${CMAKE_CURRENT_SOURCE_DIR}/assets
  CONFIGURE_DEPENDS assets/svg/*.svg)
file(STRINGS assets/icons/files.txt YAP_ICON_NAMES)
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS assets/icons/files.txt)
list(TRANSFORM YAP_ICON_NAMES PREPEND icons/ OUTPUT_VARIABLE YAP_ICON_FILES)
list(TRANSFORM YAP_FONT_FILES PREPEND assets/ OUTPUT_VARIABLE YAP_FONT_PATHS)
list(TRANSFORM YAP_SVG_FILES PREPEND assets/ OUTPUT_VARIABLE YAP_SVG_PATHS)
list(TRANSFORM YAP_ICON_FILES PREPEND assets/ OUTPUT_VARIABLE YAP_ICON_PATHS)
qt_add_resources(YapNative yap_assets
  PREFIX /
  BASE assets
  FILES ${YAP_FONT_PATHS} ${YAP_SVG_PATHS} ${YAP_ICON_PATHS} assets/app/yap-256.png)
# Avatars and server icons come as PNG, JPEG, WebP or GIF over HTTPS. Call
# video renders through Qt Multimedia's VideoOutput, whose QVideoSink needs
# a media backend (Windows Media Foundation; nothing is decoded by it).
qt_import_plugins(YapNative INCLUDE
  Qt::QSvgPlugin Qt::QGifPlugin Qt::QJpegPlugin Qt::QWebpPlugin
  Qt::QSchannelBackendPlugin Qt::QWindowsMediaPlugin)
if(WIN32)
  # dwmapi for the title bar; shell32, advapi32 and runtimeobject for the
  # tray, the Run key and WinRT toasts; the rest is what libwebrtc (static)
  # needs.
  target_link_libraries(YapNative PRIVATE
    dwmapi shell32 user32 advapi32 runtimeobject msdmo wmcodecdspuuid dmoguids crypt32 iphlpapi ole32 secur32
    winmm ws2_32 strmiids d3d11 gdi32 dxgi shcore userenv dbghelp)
endif()

# Static Qt: pull in the QML plugins the UI imports.
qt_import_qml_plugins(YapNative)
