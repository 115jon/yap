#!/usr/bin/env bash
# Builds the Qt Quick prototype for one Android ABI (an unsigned APK), or
# for the iOS simulator (an unsigned .app, a compile and link check):
#
#   desktop-native/mobile/qt/build.sh [x86_64|arm64-v8a|ios-simulator] [Release|Debug] [-D...]
#
# Arguments after the second go to CMake's configure step, e.g.
# -DYAP_ANDROID_APP_ID=..., -DYAP_ANDROID_VERSION_CODE=...,
# -DYAP_ANDROID_VERSION_NAME=... (README, "Build and run the Qt app").
#
# Needs (paths from the environment; defaults for fetch-qt.ps1's layout):
#   QT_TARGET_ROOT   the target Qt 6.11 prefix (default: $YAPM/qt/<dir>, with
#                    <dir> android_x86_64, android_arm64_v8a or ios; fetch it
#                    with fetch-qt.ps1 -Dest $YAPM/qt). QT_ANDROID_ROOT is
#                    still read for Android.
#   QT_HOST_PATH     a desktop Qt 6.11.3 with its tools (moc, qmlcachegen,
#                    androiddeployqt)
#   Android: ANDROID_SDK_ROOT (or ANDROID_HOME) and ANDROID_NDK_ROOT (default:
#   the SDK's ndk/$YAP_NDK_VERSION, 27.2.12479018, r27c, the NDK Qt 6.11.3
#   for Android was built with), JDK 17+.
#   Ninja, CMake 3.24+, and Rust with the target's standard library.
# Build trees, Gradle's cache and cargo's output live under $YAPM (default
# F:/ct/yapm), never on the system drive.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
abi="${1:-x86_64}"
config="${2:-Release}"
shift $(( $# < 2 ? $# : 2 ))
yapm="${YAPM:-F:/ct/yapm}"
case "$abi" in
  x86_64) qt_dir=android_x86_64 ;;
  arm64-v8a) qt_dir=android_arm64_v8a ;;
  ios-simulator) qt_dir=ios ;;
  *) echo "unknown target $abi (x86_64, arm64-v8a, ios-simulator)" >&2; exit 2 ;;
esac
qt="${QT_TARGET_ROOT:-${QT_ANDROID_ROOT:-$yapm/qt/$qt_dir}}"
host="${QT_HOST_PATH:?set QT_HOST_PATH to a desktop Qt 6.11}"
[[ -f "$qt/bin/target_qt.conf" ]] || { echo "no target Qt at $qt (bin/target_qt.conf)" >&2; exit 2; }

# Qt's online packages carry the Qt build machine's host paths in the qmake
# wrapper and target_qt.conf; point them at QT_HOST_PATH (what aqtinstall
# does on install). cxx-qt-build asks this qmake for the target's paths.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    host_q="$(cygpath -m "$host")"
    qt_q="$(cygpath -m "$qt")"
    printf '@echo off\r\n"%s/bin/qmake6.exe" -qtconf "%%~dp0\\target_qt.conf" %%*\r\n' "$host_q" >"$qt/bin/qmake.bat"
    cp "$qt/bin/qmake.bat" "$qt/bin/qmake6.bat"
    host_libexec=bin
    host_spec=win32-msvc
    ;;
  *)
    host_q="$host"
    qt_q="$qt"
    printf '#!/bin/sh\nexec "%s/bin/qmake" -qtconf "$(dirname "$0")/target_qt.conf" "$@"\n' "$host" >"$qt/bin/qmake"
    cp "$qt/bin/qmake" "$qt/bin/qmake6"
    chmod +x "$qt/bin/qmake" "$qt/bin/qmake6"
    # The host's tools for cxx-qt-build (moc, rcc, qmlcachegen, ...), through
    # wrappers: download.qt.io's desktop rcc compresses with zstd by default,
    # which Qt for Android cannot read (QT_FEATURE_zstd off; the link fails
    # on qt_resourceFeatureZstd). CMake's own rcc calls already pass
    # --no-zstd for such a target.
    host_libexec="$qt/yap-host-libexec"
    rm -rf "$host_libexec"
    mkdir -p "$host_libexec"
    for tool in "$host"/libexec/*; do
      [[ -f "$tool" && -x "$tool" ]] || continue
      name="$(basename "$tool")"
      extra=""
      [[ "$name" == rcc ]] && extra=" --no-zstd"
      printf '#!/bin/sh\nexec "%s"%s "$@"\n' "$tool" "$extra" >"$host_libexec/$name"
      chmod +x "$host_libexec/$name"
    done
    host_spec="$("$host/bin/qmake" -query QMAKE_SPEC)"
    ;;
esac
# -i.bak works with GNU and BSD sed alike.
sed -i.bak -E \
  -e "s#^HostPrefix=.*#HostPrefix=$host_q#" \
  -e "s#^HostData=.*#HostData=$qt_q#" \
  -e "s#^HostBinaries=.*#HostBinaries=bin#" \
  -e "s#^HostLibraryExecutables=.*#HostLibraryExecutables=$host_libexec#" \
  -e "s#^HostSpec=.*#HostSpec=$host_spec#" \
  "$qt/bin/target_qt.conf"
rm -f "$qt/bin/target_qt.conf.bak"

build="$yapm/build/qt-$abi-$config"
if [[ "$abi" == ios-simulator ]]; then
  # Qt for iOS ships static libraries for arm64 devices and x86_64
  # simulators (no arm64 simulator slice), so the simulator build is x86_64.
  # cxx-qt-build's C++ and the Rust code target Qt's minimum iOS, as the
  # link does (not the SDK's own version).
  export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
  cmake -G Ninja -S "$here" -B "$build" \
    -DCMAKE_BUILD_TYPE="$config" \
    -DCMAKE_TOOLCHAIN_FILE="$qt/lib/cmake/Qt6/qt.toolchain.cmake" \
    -DQT_HOST_PATH="$host" \
    -DCMAKE_OSX_SYSROOT=iphonesimulator \
    -DCMAKE_OSX_ARCHITECTURES=x86_64 \
    -DRust_CARGO_TARGET=x86_64-apple-ios \
    "$@"
  cmake --build "$build" --parallel "${CMAKE_BUILD_PARALLEL_LEVEL:-6}"
  find "$build" -maxdepth 2 -name '*.app' -print
  exit 0
fi

sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:?set ANDROID_SDK_ROOT}}"
ndk="${ANDROID_NDK_ROOT:-$sdk/ndk/${YAP_NDK_VERSION:-27.2.12479018}}"
[[ -d "$ndk" ]] || { echo "no NDK at $ndk: install ndk;27.2.12479018 or set ANDROID_NDK_ROOT" >&2; exit 2; }
export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$yapm/gradle}"
cmake -G Ninja -S "$here" -B "$build" \
  -DCMAKE_BUILD_TYPE="$config" \
  -DCMAKE_TOOLCHAIN_FILE="$qt/lib/cmake/Qt6/qt.toolchain.cmake" \
  -DQT_HOST_PATH="$host" \
  -DANDROID_SDK_ROOT="$sdk" \
  -DANDROID_NDK_ROOT="$ndk" \
  "$@"
cmake --build "$build" --target apk --parallel "${CMAKE_BUILD_PARALLEL_LEVEL:-6}"
find "$build/android-build" -name '*.apk' -newer "$build/build.ninja" -print
