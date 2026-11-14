# Makes rust/vendor/<crate>: a crates.io crate, the version below, with the
# patches in patches/<crate>/ applied. rust/Cargo.toml's [patch.crates-io]
# builds these copies; build.ps1 runs this when one is missing, and so must
# anyone building or testing the Rust side on a fresh checkout:
#
#   pwsh native/patches/vendor.ps1 [-Force]
#
# The copies are generated (rust/vendor is ignored by git): change a patch,
# then run with -Force. docs/LIBRARIES.md, "Patched dependencies", lists why
# each patch exists.
#
# A patch is a plain unified diff (--- a/<path>, +++ b/<path>) with no
# `diff --git` header: git apply, run inside this repository, silently skips
# the files of a git-style patch (it reads their paths from the repo root).
param([switch]$Force)
$ErrorActionPreference = "Stop"
$crates = [ordered]@{
    # qmlcachegen's bytecode-only mode for the QML files rust/build.rs names,
    # the .qml files in the resources left empty (DISCARD_QML_CONTENTS), and
    # qmlcachegen run on every core (CI time).
    "qt-build-utils" = "0.10.0"
    # No AV1 encoder or decoder (exe size); H.264 decoded by Media Foundation,
    # on the GPU where it can, into textures the UI draws as they are; VP9
    # (and VP8 where the GPU decodes it) the same way, libvpx otherwise.
    "webrtc-sys"     = "0.3.52"
    # The C++ buffer behind a native video frame, so the UI can draw the
    # decoder's D3D11 textures without a copy (zero copy, pass 6); and
    # offers that leave the receive directions alone (offer_to_receive_*
    # unset, not 0: every offer we made turned the pulled tracks inactive).
    "libwebrtc"      = "0.3.55"
}
$rust = Join-Path $PSScriptRoot "../rust"
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
foreach ($name in $crates.Keys) {
    $version = $crates[$name]
    $dest = Join-Path $rust "vendor/$name"
    $stamp = Join-Path $dest ".yap-vendored"
    $patches = @(Get-ChildItem (Join-Path $PSScriptRoot "$name/*.patch") | Sort-Object Name)
    $want = "$version " + (($patches | ForEach-Object { (Get-FileHash $_.FullName).Hash.Substring(0, 12) }) -join " ")
    if (-not $Force -and (Test-Path $stamp) -and ((Get-Content -Raw $stamp).Trim() -eq $want)) { continue }
    $find = {
        Get-ChildItem (Join-Path $cargoHome "registry/src") -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName "$name-$version" } | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    $src = & $find
    if (-not $src) {
        # Not unpacked yet: a throwaway manifest that depends on it makes
        # cargo download and unpack it.
        $tmp = Join-Path ([IO.Path]::GetTempPath()) "yap-vendor-$name"
        New-Item -ItemType Directory -Force "$tmp/src" | Out-Null
        Set-Content "$tmp/Cargo.toml" "[package]`nname = `"fetch`"`nversion = `"0.0.0`"`nedition = `"2021`"`n[dependencies]`n$name = `"=$version`"`n"
        Set-Content "$tmp/src/lib.rs" ""
        cargo fetch --manifest-path "$tmp/Cargo.toml"
        $src = & $find
        if (-not $src) { throw "$name $version is not in the cargo registry" }
    }
    if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
    Copy-Item -Recurse $src $dest
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $dest ".cargo-ok")
    Push-Location $dest
    try {
        foreach ($patch in $patches) {
            git apply --ignore-whitespace -p1 $patch.FullName
            if ($LASTEXITCODE) { throw "failed to apply $name/$($patch.Name)" }
            "applied $name/$($patch.Name)"
        }
    } finally { Pop-Location }
    Set-Content $stamp $want
}
