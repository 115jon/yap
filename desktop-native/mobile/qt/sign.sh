#!/usr/bin/env bash
# Aligns and signs an APK that build.sh made (APK Signature Scheme v2 and
# v3; minSdk 28 needs no v1 JAR signature), checks the signature, and
# writes <signed.apk>.sha256 next to it:
#
#   desktop-native/mobile/qt/sign.sh <unsigned.apk> <signed.apk>
#
# The key (defaults: the Android SDK's debug keystore):
#   YAP_KEYSTORE       ($HOME/.android/debug.keystore)
#   YAP_KEYSTORE_PASS  (android)
#   YAP_KEY_ALIAS      (androiddebugkey)
#   YAP_KEY_PASS       (the keystore's password)
# Tools: ANDROID_SDK_ROOT (or ANDROID_HOME) build-tools/$YAP_BUILD_TOOLS
# (default: the newest installed).
set -euo pipefail
in="${1:?unsigned APK}"
out="${2:?signed APK}"
sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:?set ANDROID_SDK_ROOT}}"
if [[ -n "${YAP_BUILD_TOOLS:-}" ]]; then
  bt="$sdk/build-tools/$YAP_BUILD_TOOLS"
else
  bt="$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)"
fi
ext=""
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ext=".bat" ;; esac
zipalign="$bt/zipalign"
[[ -x "$zipalign" ]] || zipalign="$bt/zipalign.exe"
apksigner="$bt/apksigner$ext"

keystore="${YAP_KEYSTORE:-$HOME/.android/debug.keystore}"
store_pass="${YAP_KEYSTORE_PASS:-android}"
alias="${YAP_KEY_ALIAS:-androiddebugkey}"
key_pass="${YAP_KEY_PASS:-$store_pass}"

tmp="$out.aligned"
"$zipalign" -f -p 4 "$in" "$tmp"
# The passwords go through the environment, not the command line.
YAP_SIGN_STORE_PASS="$store_pass" YAP_SIGN_KEY_PASS="$key_pass" "$apksigner" sign \
  --ks "$keystore" --ks-key-alias "$alias" \
  --ks-pass env:YAP_SIGN_STORE_PASS --key-pass env:YAP_SIGN_KEY_PASS \
  --v1-signing-enabled false --v2-signing-enabled true --v3-signing-enabled true \
  --v4-signing-enabled false \
  --out "$out" "$tmp"
rm -f "$tmp"
"$apksigner" verify --min-sdk-version 28 "$out"
if command -v sha256sum >/dev/null; then
  (cd "$(dirname "$out")" && sha256sum "$(basename "$out")") >"$out.sha256"
else
  (cd "$(dirname "$out")" && shasum -a 256 "$(basename "$out")") >"$out.sha256"
fi
ls -l "$out"
cat "$out.sha256"
