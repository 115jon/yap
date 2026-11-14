#!/usr/bin/env bash
# Checks that an APK's or AAB's native libraries load on 16 KB-page devices
# (Google Play's requirement for apps targeting Android 15+):
#
#   native/mobile/check-16k.sh [--elf] [--zip] <apk|aab|apks>...
#
#   --elf  every lib/<abi>/*.so's ELF LOAD segments are aligned to 16 KB or
#          more (llvm-readelf, from ANDROID_NDK_ROOT).
#   --zip  every APK's uncompressed .so starts at a 16 KB offset
#          (zipalign -c -P 16, from the SDK's build-tools/$YAP_BUILD_TOOLS,
#          default the newest). An AAB has no such offsets (bundletool
#          aligns the APKs it makes from it); an .apks (bundletool
#          build-apks) is checked APK by APK.
# Without either flag, both. Lists every library that fails, then exits 1.
set -euo pipefail
elf=0 zip=0
while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --elf) elf=1 ;;
    --zip) zip=1 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[[ $elf == 0 && $zip == 0 ]] && elf=1 zip=1
[[ $# -ge 1 ]] || { echo "usage: check-16k.sh [--elf] [--zip] <apk|aab|apks>..." >&2; exit 2; }

if [[ $elf == 1 ]]; then
  ndk="${ANDROID_NDK_ROOT:?set ANDROID_NDK_ROOT (llvm-readelf)}"
  readelf="$(ls "$ndk"/toolchains/llvm/prebuilt/*/bin/llvm-readelf* 2>/dev/null | head -n 1)"
  [[ -n "$readelf" ]] || { echo "no llvm-readelf under $ndk" >&2; exit 2; }
fi
if [[ $zip == 1 ]]; then
  sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:?set ANDROID_SDK_ROOT (zipalign)}}"
  if [[ -n "${YAP_BUILD_TOOLS:-}" ]]; then
    bt="$sdk/build-tools/$YAP_BUILD_TOOLS"
  else
    bt="$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)"
  fi
  zipalign="$bt/zipalign"
  [[ -x "$zipalign" ]] || zipalign="$bt/zipalign.exe"
fi

tmp="$(mktemp -d)"
# A Windows path for the NDK's and SDK's tools from Git Bash.
command -v cygpath >/dev/null 2>&1 && tmp="$(cygpath -m "$tmp")"
trap 'rm -rf "$tmp"' EXIT
failed=0
checked=0

check_zip() {
  local apk="$1" label="$2"
  if ! "$zipalign" -c -P 16 4 "$apk" >/dev/null; then
    # The verbose listing names the entries that are off.
    "$zipalign" -c -v -P 16 4 "$apk" | grep -E '\.so .*BAD' >&2 || true
    echo "FAIL zip: $label: a library is not at a 16 KB offset" >&2
    failed=1
  fi
}

for pkg in "$@"; do
  [[ -f "$pkg" ]] || { echo "no file $pkg" >&2; exit 2; }
  name="$(basename "$pkg")"
  if [[ $elf == 1 ]]; then
    dir="$tmp/elf-$checked"
    mkdir -p "$dir"
    # An AAB keeps the libraries under base/lib, an APK under lib. (Whole
    # packages: unzip's wildcards do not match on every host.)
    unzip -q -o "$pkg" -d "$dir"
    libs=()
    while IFS= read -r -d '' so; do libs+=("$so"); done < <(
      find "$dir/lib" "$dir/base/lib" -name '*.so' -print0 2>/dev/null)
    if [[ ${#libs[@]} -eq 0 && "$pkg" != *.apks ]]; then
      echo "FAIL elf: $name has no native libraries" >&2
      failed=1
    fi
    for so in ${libs[@]+"${libs[@]}"}; do
      rel="${so#"$dir"/}"
      # The smallest LOAD alignment (hex), e.g. 0x4000.
      align="$("$readelf" -lW "$so" | awk '$1 == "LOAD" { print $NF }' | sort -u |
        while read -r a; do printf '%d\n' "$a"; done | sort -n | head -n 1)"
      if [[ -z "$align" || "$align" -lt 16384 ]]; then
        echo "FAIL elf: $name: $rel LOAD alignment ${align:-none} < 16384" >&2
        failed=1
      fi
    done
    echo "elf: $name: ${#libs[@]} libraries checked"
    rm -rf "$dir"
  fi
  if [[ $zip == 1 ]]; then
    case "$pkg" in
      *.apk) check_zip "$pkg" "$name" ;;
      *.apks)
        dir="$tmp/apks-$checked"
        mkdir -p "$dir"
        unzip -q -o "$pkg" -d "$dir"
        n=0
        while IFS= read -r -d '' apk; do
          check_zip "$apk" "$name:${apk#"$dir"/}"
          n=$((n + 1))
        done < <(find "$dir" -name '*.apk' -print0)
        echo "zip: $name: $n APKs checked"
        rm -rf "$dir"
        ;;
    esac
  fi
  checked=$((checked + 1))
done
if [[ $failed != 0 ]]; then
  echo "16 KB check failed" >&2
  exit 1
fi
echo "16 KB check passed ($checked packages)"
