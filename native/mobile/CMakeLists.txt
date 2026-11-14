cmake_minimum_required(VERSION 3.24)
# The Qt Quick mobile prototype (native/mobile/README.md): the
# desktop's cxx-qt pattern (CMakeLists.txt one folder up) with Qt for
# Android. Configure with Qt's Android toolchain (build.sh does):
#   qt.toolchain.cmake, QT_HOST_PATH (a desktop Qt of the same version),
#   ANDROID_SDK_ROOT, ANDROID_NDK_ROOT.
# The app's version (x.y.z) lives in VERSION, which release-please bumps
# (release-please-config.json): the release workflow
# (qt-android-release.yml) and CI read the same file.
file(STRINGS ${CMAKE_CURRENT_SOURCE_DIR}/VERSION YAP_APP_VERSION LIMIT_COUNT 1)
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/VERSION)
if(NOT YAP_APP_VERSION MATCHES "^[0-9]+[.][0-9]+[.][0-9]+$")
  message(FATAL_ERROR "VERSION must be x.y.z, got '${YAP_APP_VERSION}'")
endif()
project(YapQtProto VERSION ${YAP_APP_VERSION} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# Only what the shared QML and the shell use: no Multimedia, WebView,
# Widgets, Svg widgets, Quick 3D or the Controls styles beyond Basic.
find_package(Qt6 6.11 REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2 Svg)
find_package(Qt6 REQUIRED COMPONENTS CorePrivate QuickPrivate)

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

set(DESKTOP ${CMAKE_CURRENT_SOURCE_DIR}/..)
# The C++ behind Yap.Ui (AnimatedPicture, the icon and dither providers),
# shared with the desktop; it compiles in this target.
include(${DESKTOP}/rust/crates/yap-ui/cpp/yap_ui.cmake)

qt_add_executable(yapqt
  cpp/main.cpp
  cpp/native.cpp cpp/native.h
  cpp/push.cpp
  cpp/links.cpp
  cpp/keystore.cpp
  cpp/voice.cpp
  cpp/calls.cpp
  # Remote call video: the newest frame per key and the item that draws it.
  cpp/videohub.cpp cpp/videohub.h
  cpp/videoitem.cpp cpp/videoitem.h
  cpp/rustnetwork.cpp cpp/rustnetwork.h)
target_include_directories(yapqt PRIVATE cpp)
target_link_libraries(yapqt PRIVATE
  yap_mobile_qml
  yap_ui_qml
  yap_ui_cpp
  Qt::Gui Qt::Network Qt::Quick Qt::QuickControls2 Qt::Svg Qt::CorePrivate Qt::QuickPrivate)

# --- Voice: libwebrtc on Android --------------------------------------------
# The Rust library carries LiveKit's prebuilt WebRTC (yap-voice). Its Java
# side and the names Java binds to come out of yap-voice's build script
# (rust/crates/yap-voice/build.rs, YAP_WEBRTC_EXPORT_DIR):
#   libwebrtc.jar    livekit.org.webrtc.*, copied where Gradle picks up jars
#                    (android/build.gradle: libs/*.jar);
#   webrtc_jni.rsp   -Wl,--undefined=Java_livekit_org_webrtc_... for every
#                    JNI entry point, which nothing native refers to: without
#                    it the link drops them and the audio device module's
#                    first Java-to-native call fails.
# The NDK libraries are WebRTC's (OpenSL ES and AAudio audio, EGL for its
# video); libc++ is the shared one Qt already loads.
if(ANDROID)
  set(YAP_WEBRTC_EXPORT ${CMAKE_CURRENT_BINARY_DIR}/webrtc)
  if(NOT CMAKE_ANDROID_NDK)
    set(CMAKE_ANDROID_NDK "${ANDROID_NDK_ROOT}")
  endif()
  corrosion_set_env_vars(yap_mobile_qt
    "YAP_WEBRTC_EXPORT_DIR=${YAP_WEBRTC_EXPORT}"
    # webrtc-sys looks for the NDK here (its sysroot and llvm tools).
    "ANDROID_NDK_HOME=${CMAKE_ANDROID_NDK}")
  target_link_options(yapqt PRIVATE "@${YAP_WEBRTC_EXPORT}/webrtc_jni.rsp")
  target_link_libraries(yapqt PRIVATE OpenSLES aaudio EGL GLESv2 android log)
  add_custom_command(TARGET yapqt POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory ${CMAKE_CURRENT_BINARY_DIR}/android-build/libs
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
      ${YAP_WEBRTC_EXPORT}/libwebrtc.jar ${CMAKE_CURRENT_BINARY_DIR}/android-build/libs/libwebrtc.jar
    VERBATIM)

  # The third-party notices and licence texts, as APK assets
  # (assets/licenses; Gradle packs android-build/assets): Settings > About >
  # Open source licences reads them (Native.licenseFiles). Gathered on every
  # build, for this ABI's crates (tools/gather-licenses.ps1, PowerShell 7,
  # which build.sh already needs for patches/vendor.ps1).
  find_program(YAP_PWSH pwsh REQUIRED)
  add_custom_target(yap_licenses
    COMMAND ${YAP_PWSH} -NoProfile -File ${DESKTOP}/tools/gather-licenses.ps1
      -Out ${CMAKE_CURRENT_BINARY_DIR}/android-build/assets/licenses
      -Manifest ${CMAKE_CURRENT_SOURCE_DIR}/rust/Cargo.toml
      -CargoTarget ${Rust_CARGO_TARGET_CACHED}
      -Cargo ${Rust_CARGO_CACHED}
    VERBATIM)
  add_dependencies(yapqt yap_licenses)
endif()

# The application id and version: the packaging block at the end.
set_target_properties(yapqt PROPERTIES
  QT_ANDROID_PACKAGE_SOURCE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/android
  QT_ANDROID_MIN_SDK_VERSION 28
  QT_ANDROID_TARGET_SDK_VERSION 36
  # Where androiddeployqt's import scan looks for the QML modules to
  # bundle: this app's and the shared Yap.Ui's, which lives in yap-ui.
  QT_QML_ROOT_PATH "${CMAKE_CURRENT_SOURCE_DIR};${DESKTOP}/rust/crates/yap-ui/qml")

# The desktop's brand fonts and Phosphor icons (native/assets),
# embedded as :/fonts and :/icons as on the desktop.
file(GLOB YAP_FONT_FILES RELATIVE ${DESKTOP}/assets CONFIGURE_DEPENDS ${DESKTOP}/assets/fonts/*.ttf)
file(STRINGS ${DESKTOP}/assets/icons/files.txt YAP_ICON_NAMES)
list(TRANSFORM YAP_ICON_NAMES PREPEND icons/ OUTPUT_VARIABLE YAP_ICON_FILES)
# Home's Yappy mark on the rail: the plain themes' (Theme.homeMark) and the
# brand themes' own (Theme.themeMark: Miku, Spider-Man).
set(YAP_ASSET_FILES ${YAP_ICON_FILES}
  svg/home-light.svg svg/home-dark.svg
  svg/home-miku-light.svg svg/home-miku-dark.svg
  svg/home-spiderman-light.svg svg/home-spiderman-dark.svg
  # The GIF picker's "Powered by" KLIPY wordmark (Yap.Ui ProviderMark).
  svg/klipy-text-light.svg)
list(TRANSFORM YAP_ASSET_FILES PREPEND ${DESKTOP}/assets/ OUTPUT_VARIABLE YAP_ASSET_PATHS)
qt_add_resources(yapqt yap_assets
  PREFIX /
  BASE ${DESKTOP}/assets
  FILES ${YAP_ASSET_PATHS})
# The fonts are stored as they are (2 MB; rcc's default threshold left them
# uncompressed already, -no-compress makes that a rule): main.cpp hands Qt
# the mapped bytes, so no font is copied onto the heap.
list(TRANSFORM YAP_FONT_FILES PREPEND ${DESKTOP}/assets/ OUTPUT_VARIABLE YAP_FONT_PATHS)
qt_add_resources(yapqt yap_fonts
  PREFIX /
  BASE ${DESKTOP}/assets
  FILES ${YAP_FONT_PATHS}
  OPTIONS -no-compress)

# Only the plugins the app uses (androiddeployqt otherwise ships every one
# of each module): GIF, JPEG, WebP and SVG pictures; no QML debugger or
# profiler services, no icon engines or network-information backends.
qt_import_plugins(yapqt
  INCLUDE_BY_TYPE imageformats Qt::QGifPlugin Qt::QJpegPlugin Qt::QWebpPlugin Qt::QSvgPlugin
  EXCLUDE_BY_TYPE qmltooling iconengines networkinformation)

# --- Packaging: application id and version -------------------------------
# CI passes the version per run (native-mobile.yml: code = run * 10
# + an ABI digit, name <VERSION>-ci.<run>+<commit>; qt-android-release.yml:
# code = (2000000 + minutes since 2026-01-01 UTC) * 10 + the digit, name
# <VERSION>); local builds get <VERSION> (1).
# The id is the Gradle namespace and application id (androiddeployqt writes
# it to gradle.properties), and the iOS bundle identifier. dev.yap.app is
# the app's own, for release builds and local ones (registered in Firebase
# for push notifications; Gradle reads its client from
# YAP_GOOGLE_SERVICES_JSON, android/build.gradle). CI's builds, signed with
# the public CI key, are dev.yap.app.ci, labelled "Yap CI": anyone holding
# that key could sign an update to them, so they never share the release
# app's id (README, "Signing").
set(YAP_ANDROID_APP_ID "dev.yap.app" CACHE STRING "Android application id (and iOS bundle id)")
set(YAP_ANDROID_APP_NAME "Yap" CACHE STRING "The app's label on the launcher")
set(YAP_ANDROID_VERSION_CODE "1" CACHE STRING "Android versionCode")
set(YAP_ANDROID_VERSION_NAME "" CACHE STRING "Android versionName (empty: VERSION)")
if(NOT YAP_ANDROID_VERSION_NAME)
  set(YAP_ANDROID_VERSION_NAME "${YAP_APP_VERSION}")
endif()
set_target_properties(yapqt PROPERTIES
  QT_ANDROID_APP_NAME "${YAP_ANDROID_APP_NAME}"
  QT_ANDROID_PACKAGE_NAME "${YAP_ANDROID_APP_ID}"
  QT_ANDROID_VERSION_CODE "${YAP_ANDROID_VERSION_CODE}"
  QT_ANDROID_VERSION_NAME "${YAP_ANDROID_VERSION_NAME}")
# The SDK platform and build tools androiddeployqt compiles with; empty
# takes the newest installed. CI pins both, so the runner image's newest
# platform does not decide.
set(YAP_ANDROID_COMPILE_SDK "" CACHE STRING "Android SDK platform to compile with (e.g. android-36)")
set(YAP_ANDROID_BUILD_TOOLS "" CACHE STRING "Android SDK build-tools revision (e.g. 36.0.0)")
if(YAP_ANDROID_COMPILE_SDK)
  set_target_properties(yapqt PROPERTIES QT_ANDROID_COMPILE_SDK_VERSION "${YAP_ANDROID_COMPILE_SDK}")
endif()
if(YAP_ANDROID_BUILD_TOOLS)
  set_target_properties(yapqt PROPERTIES QT_ANDROID_SDK_BUILD_TOOLS_REVISION "${YAP_ANDROID_BUILD_TOOLS}")
endif()

# --- Size: plugins nobody loads ------------------------------------------
# The offscreen platform plugin (the android one draws) and the OpenSSL TLS
# backend. Qt for Android ships no libssl, so the backend never loaded;
# network pictures go through the core's rustls client instead
# (cpp/rustnetwork.cpp, README "Gaps found").
qt_import_plugins(yapqt
  EXCLUDE Qt::QOffscreenIntegrationPlugin Qt::QTlsBackendOpenSSLPlugin)

# --- Size: link flags ------------------------------------------------------
# The NDK's toolchain already links with --gc-sections, and the APK's
# libraries are stripped by Gradle. Identical code folding saves 0.3 MB on
# arm64 (measured; --icf=safe saved nothing, as lld then keeps every
# function whose address might be taken). Qt runs folded the same way on
# Windows, where MSVC's release /OPT:ICF folds alike.
if(ANDROID)
  target_link_options(yapqt PRIVATE "$<$<NOT:$<CONFIG:Debug>>:LINKER:--icf=all>")
endif()

# --- iOS: a compile and link check for the simulator ---------------------
# (build.sh ios-simulator; CI's qt-ios job.) Qt for iOS is static, so the
# QML modules' plugins are linked in: qmlimportscanner reads the mobile QML
# under this folder, the shared Yap.Ui QML (yap-ui) and the desktop QML
# (desktop-only modules such as QtMultimedia are not in this Qt and are
# skipped). Qt writes a default Info.plist from these properties.
if(IOS)
  set_property(TARGET yapqt APPEND PROPERTY QT_QML_IMPORT_SCANNER_EXTRA_ROOT_PATHS
    ${DESKTOP}/rust/crates/yap-ui/qml ${DESKTOP}/rust/qml)
  qt_import_qml_plugins(yapqt)
  set_target_properties(yapqt PROPERTIES
    MACOSX_BUNDLE TRUE
    MACOSX_BUNDLE_GUI_IDENTIFIER "${YAP_ANDROID_APP_ID}"
    MACOSX_BUNDLE_BUNDLE_NAME "Yap"
    MACOSX_BUNDLE_SHORT_VERSION_STRING "${YAP_ANDROID_VERSION_NAME}"
    MACOSX_BUNDLE_BUNDLE_VERSION "${YAP_ANDROID_VERSION_CODE}"
    XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER "${YAP_ANDROID_APP_ID}"
    XCODE_ATTRIBUTE_CODE_SIGNING_ALLOWED NO)
endif()
