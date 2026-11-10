#!/usr/bin/env bash
# Builds the prototype: the Rust core for Android (cargo-ndk), its Kotlin
# bindings (uniffi-bindgen, read from the built library), then the APK.
#
#   desktop-native/mobile/build.sh [x86_64|arm64-v8a|all] [debug|release]
#
# Needs: rustup targets x86_64-linux-android / aarch64-linux-android,
# cargo-ndk, an NDK (ANDROID_NDK_HOME, else the newest under the SDK), and
# JDK 17+. The APK lands in android/app/build/outputs/apk/.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
abi="${1:-x86_64}"
profile="${2:-release}"
sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$LOCALAPPDATA/Android/Sdk}}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="$(ls -d "$sdk"/ndk/* | sort -V | tail -1)"
  export ANDROID_NDK_HOME
fi
targets=(-t x86_64)
[[ "$abi" == "arm64-v8a" ]] && targets=(-t arm64-v8a)
[[ "$abi" == "all" ]] && targets=(-t x86_64 -t arm64-v8a)
cargo_profile=(); [[ "$profile" == "release" ]] && cargo_profile=(--release)

cd "$here/rust"
cargo ndk "${targets[@]}" -P 26 -o "$here/android/app/src/main/jniLibs" build "${cargo_profile[@]}" --lib
# cargo-ndk copies every cdylib in the graph (rusty-leveldb builds one);
# the app needs only ours.
find "$here/android/app/src/main/jniLibs" -name '*.so' ! -name libyap_mobile.so -delete

# Bindings come from a host build: the release .so has its symbol table
# (and so UniFFI's metadata) stripped.
cargo build -q --lib
host_lib=""
for f in target/debug/libyap_mobile.so target/debug/libyap_mobile.dylib target/debug/yap_mobile.dll; do
  [[ -f "$f" ]] && host_lib="$f" && break
done
cargo run -q --bin uniffi-bindgen -- generate --library "$host_lib" --language kotlin \
  --out-dir "$here/android/app/src/main/java" --no-format

cd "$here/android"
gradle_task=assembleRelease; [[ "$profile" == "debug" ]] && gradle_task=assembleDebug
# Through sh: a Windows checkout does not keep the executable bit.
sh ./gradlew -q "$gradle_task"
