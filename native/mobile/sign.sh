#!/usr/bin/env bash
# Aligns and signs an APK that build.sh made (APK Signature Scheme v2 and
# v3; minSdk 28 needs no v1 JAR signature), or signs an App Bundle that
# bundle.sh made (a JAR signature, jarsigner: what Google Play reads from an
# upload), checks the signature, and writes <signed>.sha256 next to it:
#
#   native/mobile/sign.sh <unsigned.apk> <signed.apk>
#   native/mobile/sign.sh <unsigned.aab> <signed.aab>
#
# APKs are aligned for 16 KB pages: each uncompressed native library at a
# 16 KB offset (zipalign -P 16, build tools 35+), checked after signing.
#
# The key (defaults: the Android SDK's debug keystore):
#   YAP_KEYSTORE       ($HOME/.android/debug.keystore)
#   YAP_KEYSTORE_PASS  (android)
#   YAP_KEY_ALIAS      (androiddebugkey)
#   YAP_KEY_PASS       (the keystore's password)
# Tools: APKs, ANDROID_SDK_ROOT (or ANDROID_HOME) build-tools/$YAP_BUILD_TOOLS
# (default: the newest installed); AABs, the JDK's jarsigner (JAVA_HOME's,
# else PATH's).
set -euo pipefail
in="${1:?unsigned APK or AAB}"
out="${2:?signed APK or AAB}"

keystore="${YAP_KEYSTORE:-$HOME/.android/debug.keystore}"
store_pass="${YAP_KEYSTORE_PASS:-android}"
alias="${YAP_KEY_ALIAS:-androiddebugkey}"
key_pass="${YAP_KEY_PASS:-$store_pass}"

checksum() {
  if command -v sha256sum >/dev/null; then
    (cd "$(dirname "$1")" && sha256sum "$(basename "$1")") >"$1.sha256"
  else
    (cd "$(dirname "$1")" && shasum -a 256 "$(basename "$1")") >"$1.sha256"
  fi
  ls -l "$1"
  cat "$1.sha256"
}

if [[ "$in" == *.aab ]]; then
  [[ "$out" == *.aab ]] || { echo "sign.sh: an AAB signs to an .aab" >&2; exit 2; }
  jarsigner="${JAVA_HOME:+$JAVA_HOME/bin/}jarsigner"
  # jarsigner's default algorithms for the key (SHA-256 digests); the
  # passwords go through the environment.
  YAP_SIGN_STORE_PASS="$store_pass" YAP_SIGN_KEY_PASS="$key_pass" "$jarsigner" \
    -keystore "$keystore" -storepass:env YAP_SIGN_STORE_PASS -keypass:env YAP_SIGN_KEY_PASS \
    -signedjar "$out" "$in" "$alias" >/dev/null
  # Warnings about a self-signed certificate without a timestamp are
  # expected (Android's signing keys are self-signed).
  verify="$("$jarsigner" -verify "$out")"
  if ! grep -qx 'jar verified.' <<<"$verify"; then
    echo "$verify" >&2
    echo "sign.sh: $out did not verify" >&2
    exit 1
  fi
  echo "jar verified: $out"
  checksum "$out"
  exit 0
fi

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

tmp="$out.aligned"
"$zipalign" -f -P 16 4 "$in" "$tmp"
# The passwords go through the environment, not the command line.
YAP_SIGN_STORE_PASS="$store_pass" YAP_SIGN_KEY_PASS="$key_pass" "$apksigner" sign \
  --ks "$keystore" --ks-key-alias "$alias" \
  --ks-pass env:YAP_SIGN_STORE_PASS --key-pass env:YAP_SIGN_KEY_PASS \
  --v1-signing-enabled false --v2-signing-enabled true --v3-signing-enabled true \
  --v4-signing-enabled false \
  --out "$out" "$tmp"
rm -f "$tmp"
"$apksigner" verify --min-sdk-version 28 "$out"
"$zipalign" -c -P 16 4 "$out"
checksum "$out"
