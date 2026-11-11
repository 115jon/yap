#!/usr/bin/env bash
# Installs a signed APK on a running emulator or device, cold-starts it and
# waits for the app's "[startup] first frame" log line (main.cpp): the Qt
# runtime, the QML and the core's start-up all loaded. No network needed.
#
#   desktop-native/mobile/qt/smoke.sh <apk> [timeout seconds, default 120]
#
# ADB_SERIAL picks the device (default: adb's only one). Uninstalls the app
# afterwards.
set -euo pipefail
apk="${1:?APK}"
timeout="${2:-120}"
adb=(adb)
[[ -n "${ADB_SERIAL:-}" ]] && adb+=(-s "$ADB_SERIAL")
pkg=dev.yap.qtproto
activity=org.qtproject.qt.android.bindings.QtActivity

"${adb[@]}" wait-for-device
"${adb[@]}" install -r "$apk"
"${adb[@]}" logcat -c
"${adb[@]}" shell am start -W -n "$pkg/$activity"
status=1
for ((i = 0; i < timeout; i += 2)); do
  if "${adb[@]}" logcat -d | grep -q '\[startup\] first frame'; then
    status=0
    break
  fi
  sleep 2
done
pid="$("${adb[@]}" shell pidof "$pkg" | tr -d '\r' || true)"
if ((status)); then
  # The app's own log (Qt's tags, the core's yap-core), and crashes.
  echo "== app process: ${pid:-not running}"
  "${adb[@]}" logcat -d | grep -E " ${pid:-NOPID} .* [VDIWEF] |FATAL|AndroidRuntime|DEBUG *:|ActivityManager: .*$pkg" | tail -80 || true
else
  "${adb[@]}" logcat -d | grep -E '\[startup\]|\[cache\]' || true
fi
"${adb[@]}" uninstall "$pkg" >/dev/null || true
if ((status)); then
  echo "no '[startup] first frame' within ${timeout}s" >&2
else
  echo "smoke ok: first frame"
fi
exit "$status"
