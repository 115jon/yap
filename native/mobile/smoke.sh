#!/usr/bin/env bash
# Installs a signed APK on a running emulator or device, cold-starts it and
# waits for the app's "[startup] first frame" log line (main.cpp): the Qt
# runtime, the QML and the core's start-up all loaded. No network needed.
#
#   native/mobile/smoke.sh <apk> [timeout seconds, default 120]
#
# ADB_SERIAL picks the device (default: adb's only one); YAP_ANDROID_APP_ID
# is the APK's application id (default dev.yap.app; CI's builds are
# dev.yap.app.ci). Uninstalls the app afterwards.
set -euo pipefail
apk="${1:?APK}"
timeout="${2:-120}"
adb=(adb)
[[ -n "${ADB_SERIAL:-}" ]] && adb+=(-s "$ADB_SERIAL")
pkg="${YAP_ANDROID_APP_ID:-dev.yap.app}"
activity=dev.yap.qtproto.YapActivity

"${adb[@]}" wait-for-device
until [[ "$("${adb[@]}" shell getprop sys.boot_completed | tr -d '\r')" == 1 ]]; do sleep 2; done
"${adb[@]}" install -r "$apk"
# Two cold starts at most, half the time each: a freshly booted CI emulator
# has once left the first launch without a frame or a log line.
status=1
for attempt in 1 2; do
  "${adb[@]}" shell am force-stop "$pkg"
  "${adb[@]}" logcat -c
  "${adb[@]}" shell am start -W -n "$pkg/$activity"
  for ((i = 0; i < timeout / 2; i += 2)); do
    if "${adb[@]}" logcat -d | grep -q '\[startup\] first frame'; then
      status=0
      break 2
    fi
    sleep 2
  done
  echo "attempt $attempt: no first frame within $((timeout / 2))s"
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
