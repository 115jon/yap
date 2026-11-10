#!/usr/bin/env bash
# Builds the Qt Quick prototype's APK for one ABI:
#
#   desktop-native/mobile/qt/build.sh [x86_64|arm64-v8a] [Release|Debug]
#
# Needs (paths from the environment, defaults for this machine's layout):
#   QT_ANDROID_ROOT  Qt 6.11 for Android, the ABI's prefix root
#                    (default: $YAPM/qt/<abi>, from download.qt.io)
#   QT_HOST_PATH     a desktop Qt 6.11 with its tools (moc, qmlcachegen,
#                    androiddeployqt)
#   ANDROID_SDK_ROOT, ANDROID_NDK_ROOT (default: the newest NDK in the SDK)
#   JDK 17+, Ninja, a Rust toolchain with the Android target.
# Build trees, Gradle's cache and cargo's target live under $YAPM (default
# F:/ct/yapm), never on the system drive.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
abi="${1:-x86_64}"
config="${2:-Release}"
yapm="${YAPM:-F:/ct/yapm}"
case "$abi" in
  x86_64) qt_abi=x86_64 ;;
  arm64-v8a) qt_abi=arm64_v8a ;;
  *) echo "unknown ABI $abi" >&2; exit 2 ;;
esac
qt="${QT_ANDROID_ROOT:-$yapm/qt/$qt_abi}"
host="${QT_HOST_PATH:?set QT_HOST_PATH to a desktop Qt 6.11}"
sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:?set ANDROID_SDK_ROOT}}"
if [[ -z "${ANDROID_NDK_ROOT:-}" ]]; then
  ANDROID_NDK_ROOT="$(ls -d "$sdk"/ndk/* | sort -V | tail -1)"
fi
export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$yapm/gradle}"

# Qt's online packages carry the build machine's host paths in the qmake
# wrapper and target_qt.conf; point them at QT_HOST_PATH (what aqtinstall
# does on install). cxx-qt-build asks this qmake for the target's paths.
host_win="$(cygpath -m "$host" 2>/dev/null || echo "$host")"
qt_win="$(cygpath -m "$qt" 2>/dev/null || echo "$qt")"
printf '@echo off\r\n"%s/bin/qmake6.exe" -qtconf "%%~dp0\\target_qt.conf" %%*\r\n' "$host_win" >"$qt/bin/qmake.bat"
cp "$qt/bin/qmake.bat" "$qt/bin/qmake6.bat"
sed -i -E \
  -e "s#^HostPrefix=.*#HostPrefix=$host_win#" \
  -e "s#^HostData=.*#HostData=$qt_win#" \
  -e "s#^HostBinaries=.*#HostBinaries=bin#" \
  -e "s#^HostLibraryExecutables=.*#HostLibraryExecutables=bin#" \
  -e "s#^HostSpec=.*#HostSpec=win32-msvc#" \
  "$qt/bin/target_qt.conf"

build="$yapm/build/qt-$abi-$config"
cmake -G Ninja -S "$here" -B "$build" \
  -DCMAKE_BUILD_TYPE="$config" \
  -DCMAKE_TOOLCHAIN_FILE="$qt/lib/cmake/Qt6/qt.toolchain.cmake" \
  -DQT_HOST_PATH="$host" \
  -DANDROID_SDK_ROOT="$sdk" \
  -DANDROID_NDK_ROOT="$ANDROID_NDK_ROOT"
cmake --build "$build" --target apk
find "$build/android-build" -name '*.apk' -newer "$build/build.ninja" -print
