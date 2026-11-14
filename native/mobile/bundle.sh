#!/usr/bin/env bash
# The Android App Bundle (AAB) for Google Play, holding every ABI under one
# version code, from the per-ABI builds build.sh makes. Each ABI builds in
# its own tree (and, on CI, its own job); this script puts their Gradle
# projects together and lets Gradle bundle the result:
#
#   native/mobile/bundle.sh stage <build tree> <stage dir>
#   native/mobile/bundle.sh merge <unsigned.aab> <stage dir>...
#
# stage copies one ABI's Gradle project (<build tree>/android-build, as
# androiddeployqt left it after build.sh) into <stage dir>, without Gradle's
# outputs, and with the target Qt's Java sources (gradle.properties'
# qtAndroidDir) inside it, so a stage moves between machines (CI uploads it
# as an artifact).
#
# merge starts from the first stage, adds the other stages' native
# libraries (libs/<abi>), joins the Qt library lists androiddeployqt wrote
# per ABI (res/values/libs.xml) and the ABI list (gradle.properties), and
# runs Gradle's bundleRelease. The stages must come from one commit and one
# configuration: the manifest (application id, version code and name), the
# Java and the other assets must be the same. The licence notices are
# joined (each ABI lists its own crates). The AAB is unsigned; sign.sh signs
# it.
#
# Needs: ANDROID_SDK_ROOT (or ANDROID_HOME) with the stages' platform and
# build tools, the NDK the stages name (Gradle strips the libraries with
# it), JDK 17+, python3. Like build.sh: YAP_GOOGLE_SERVICES_JSON for push
# notifications (android/build.gradle), GRADLE_USER_HOME under $YAPM.
set -euo pipefail

usage() {
  echo "usage: bundle.sh stage <build tree> <stage dir> | merge <unsigned.aab> <stage dir>..." >&2
  exit 2
}

# The one ABI folder under a Gradle project's libs.
abi_of() {
  local dirs=("$1"/libs/*/)
  if [[ ${#dirs[@]} -ne 1 || ! -d "${dirs[0]}" ]]; then
    echo "$1: expected one ABI under libs/, found: ${dirs[*]}" >&2
    exit 1
  fi
  basename "${dirs[0]}"
}

prop() { sed -n "s/^$2=//p" "$1/gradle.properties" | tail -n 1; }

cmd="${1:-}"
[[ $# -ge 1 ]] && shift
case "$cmd" in
  stage)
    [[ $# -eq 2 ]] || usage
    src="$1/android-build"
    stage="$2"
    [[ -f "$src/AndroidManifest.xml" && -f "$src/gradle.properties" ]] ||
      { echo "no Gradle project at $src (build.sh first)" >&2; exit 1; }
    abi="$(abi_of "$src")"
    qt_java="$(prop "$src" qtAndroidDir)"
    [[ -d "$qt_java/src" ]] || { echo "qtAndroidDir $qt_java has no src/" >&2; exit 1; }
    rm -rf "$stage"
    mkdir -p "$stage"
    tar -C "$src" \
      --exclude=./build --exclude=./.gradle --exclude=./local.properties \
      --exclude='./*.apk' --exclude='./*.aab' --exclude='./*.d' \
      -cf - . | tar -C "$stage" -xf -
    mkdir -p "$stage/qt-android-java"
    cp -R "$qt_java/." "$stage/qt-android-java/"
    # A path relative to the project (build.gradle appends /src, /res).
    sed -i.bak -E \
      -e 's#^qtAndroidDir=.*#qtAndroidDir=qt-android-java#' \
      -e 's#^qt5AndroidDir=.*#qt5AndroidDir=qt-android-java#' \
      "$stage/gradle.properties"
    rm -f "$stage/gradle.properties.bak"
    echo "staged $abi: $stage ($(du -sh "$stage" | cut -f 1))"
    ;;

  merge)
    [[ $# -ge 2 ]] || usage
    out="$1"
    shift
    first="$1"
    sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:?set ANDROID_SDK_ROOT}}"
    command -v cygpath >/dev/null 2>&1 && sdk="$(cygpath -m "$sdk")"
    work="${out%.aab}.gradle"
    rm -rf "$work"
    mkdir -p "$work"
    cp -R "$first/." "$work/"
    abis=("$(abi_of "$first")")
    for stage in "${@:2}"; do
      abi="$(abi_of "$stage")"
      for seen in "${abis[@]}"; do
        [[ "$seen" != "$abi" ]] || { echo "$abi is in two stages" >&2; exit 1; }
      done
      # Everything but this ABI's libraries, the library list, the ABI list
      # and the licence notices must be the first stage's.
      if ! diff -r -q \
        --exclude="$abi" --exclude="${abis[0]}" --exclude=libs.xml \
        --exclude=gradle.properties --exclude=licenses \
        "$first" "$stage"; then
        echo "$stage differs from $first beyond its ABI (one commit and configuration?)" >&2
        exit 1
      fi
      if ! diff <(grep -v '^qtTargetAbiList=' "$first/gradle.properties") \
        <(grep -v '^qtTargetAbiList=' "$stage/gradle.properties"); then
        echo "$stage: gradle.properties differs from $first's" >&2
        exit 1
      fi
      cp -R "$stage/libs/$abi" "$work/libs/$abi"
      cp -R "$stage/assets/licenses/." "$work/assets/licenses/"
      abis+=("$abi")
    done
    # libs.xml: each array's items in stage order, without repeats.
    python3 -I - "$work/res/values/libs.xml" "${@/%//res/values/libs.xml}" <<'PY'
import sys
import xml.etree.ElementTree as ET

out, inputs = sys.argv[1], sys.argv[2:]
tree = ET.parse(inputs[0])
root = tree.getroot()
for other in inputs[1:]:
    for array in ET.parse(other).getroot().iter("array"):
        mine = root.find(f"array[@name='{array.get('name')}']")
        if mine is None:
            sys.exit(f"{other}: array {array.get('name')} is not in {inputs[0]}")
        have = [item.text for item in mine.findall("item")]
        for item in array.findall("item"):
            if item.text not in have:
                ET.SubElement(mine, "item").text = item.text
                have.append(item.text)
ET.indent(tree, space="    ")
tree.write(out, encoding="utf-8", xml_declaration=True)
PY
    list="$(IFS=,; echo "${abis[*]}")"
    sed -i.bak -E "s#^qtTargetAbiList=.*#qtTargetAbiList=$list#" "$work/gradle.properties"
    rm -f "$work/gradle.properties.bak"
    printf 'sdk.dir=%s\n' "$sdk" >"$work/local.properties"

    yapm="${YAPM:-F:/ct/yapm}"
    export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$yapm/gradle}"
    (cd "$work" && chmod +x gradlew && ./gradlew --no-daemon --console=plain bundleRelease)
    aab="$(find "$work/build/outputs/bundle/release" -name '*.aab' | head -n 1)"
    [[ -n "$aab" ]] || { echo "Gradle made no AAB" >&2; exit 1; }
    mkdir -p "$(dirname "$out")"
    cp "$aab" "$out"
    echo "AAB ($list): $out"
    ls -l "$out"
    ;;

  *) usage ;;
esac
