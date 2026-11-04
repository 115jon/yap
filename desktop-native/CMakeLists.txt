cmake_minimum_required(VERSION 3.24)
project(YapNative VERSION 0.1.0 LANGUAGES CXX)

# The static Qt build uses the static MSVC runtime (/MT); the Rust side
# matches it through rust/.cargo/config.toml (+crt-static).
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS
  Core Gui Network Qml Quick QuickControls2 QmlImportScanner)

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

qt_add_executable(YapNative WIN32 cpp/main.cpp)
target_link_libraries(YapNative PRIVATE
  yap_native_qml
  Qt::Gui Qt::Network Qt::Quick Qt::QuickControls2
  $<$<PLATFORM_ID:Windows>:dwmapi>)

# Static Qt: pull in the QML plugins the UI imports.
qt_import_qml_plugins(YapNative)
