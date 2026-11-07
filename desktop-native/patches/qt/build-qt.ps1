# Builds the static Qt the native client links (docs/LIBRARIES.md, "Custom
# Qt"): Qt 6.11.3 from the qt-everywhere source zip, only the modules the
# app uses, optimized for size, with the patches in this folder.
#
#   pwsh build-qt.ps1 [-Zip <qt-everywhere-src-6.11.3.zip>] [-Root F:/ct/qt-custom-6.11] [-Clean]
#   pwsh build-qt.ps1 -Version 6.6.3 -Zip <...6.6.3.zip> -Root F:/ct/qt-custom   (the old build)
#
# Patches: <Version>/*.patch (6.6.3's sit in this folder).
#
# Output: <Root>/<Version>-yap (build.ps1's default, or pass it as -QtDir /
# YAP_QT_DIR). Intermediates: <Root>/src and <Root>/build, removed with
# -Clean once installed. Needs Visual Studio 2022 (the MSVC the stock
# static Qt was built with), CMake, Ninja and Python; ~30 GB while it
# builds, ~8 GB installed (LTCG libraries).
#
# Differences from a default static Qt (both versions):
#   -optimize-size            /O1 for Qt itself (exe size; docs/PERFORMANCE.md)
#   -no-opengl -no-feature-vulkan   Qt Quick runs on D3D11 only
#   -no-freetype -no-widgets -no-dbus -no-feature-sql -no-openssl
#                             unused here (Schannel is the TLS backend)
#   -no-feature-ffmpeg        Qt Multimedia's Windows Media Foundation backend only
# 6.11.3 only (performance pass 5):
#   -no-feature-freetype      6.11 builds FreeType despite -no-freetype; the app
#                             draws with DirectWrite
#   -no-feature-timezone_locale   CLDR time-zone display names (3.75 MiB); the
#                             app formats dates in Rust and never shows a zone name
#   other unused features     calendars, PDF/ODF/Markdown writers, shared memory,
#                             SOCKS5, DNS lookups, the Network List Manager monitor,
#                             TableView/TreeView, sprites, particles, XMLHttpRequest,
#                             WorkerScript, PPM and XBM images, the unused Controls
#                             styles (XPM stays: drag and drop, so DropArea, needs it)
#   -DQT_YAP_HLSL_ONLY=ON     with 0002: Qt's own shaders carry SPIR-V and DXBC only
# Patches: 6.6.3's 0001 frees the batch renderer's pooled batches and glyph
# caches no text uses (the Settings memory, PERFORMANCE.md); in 6.11.3 the
# batch half is upstream, 0001 is the glyph half (both render loops), 0002
# the shader one and 0003 lets the Windows time zones build without CLDR names.
param(
    [string]$Version = "6.11.3",
    [string]$Zip = $(if ($Version -eq "6.6.3") { "F:/Files/Downloads/qt/src/qt-everywhere-src-6.6.3.zip" } else { "F:/ct/qt-dl/qt-everywhere-src-$Version.zip" }),
    [string]$Root = $(if ($Version -eq "6.6.3") { "F:/ct/qt-custom" } else { "F:/ct/qt-custom-6.11" }),
    [switch]$Clean
)
$ErrorActionPreference = "Stop"
$version = $Version
$patches = if ($version -eq "6.6.3") { "$PSScriptRoot/*.patch" } else { "$PSScriptRoot/$version/*.patch" }
$src = "$Root/src/qt-everywhere-src-$version"
$build = "$Root/build"
$prefix = "$Root/$version-yap"
$modules = "qtbase", "qtshadertools", "qtdeclarative", "qtsvg", "qtimageformats", "qtmultimedia"

if (-not (Test-Path "$src/qtbase")) {
    New-Item -ItemType Directory -Force "$Root/src" | Out-Null
    $top = "qt-everywhere-src-$version"
    $want = @("$top/.gitmodules", "$top/CMakeLists.txt", "$top/configure.bat", "$top/configure",
              "$top/configure.json", "$top/cmake/*") + ($modules | ForEach-Object { "$top/$_/*" })
    # Windows' bsdtar reads zips (Git's GNU tar on PATH does not).
    & "$env:SystemRoot/System32/tar.exe" -xf $Zip -C "$Root/src" @want
    if ($LASTEXITCODE) { throw "extracting $Zip failed" }
}
if (-not (Test-Path "$src/.yap-patched")) {
    Push-Location $src
    try {
        foreach ($patch in Get-ChildItem $patches | Sort-Object Name) {
            git apply --ignore-whitespace -p1 $patch.FullName
            if ($LASTEXITCODE) { throw "failed to apply $($patch.Name)" }
            "applied $($patch.Name)"
        }
        New-Item -ItemType File "$src/.yap-patched" | Out-Null
    } finally { Pop-Location }
}

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -version "[17.0,18.0)" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
if (-not $vs) { throw "Visual Studio 2022 with the x64 C++ tools not found" }
Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null

New-Item -ItemType Directory -Force $build | Out-Null
# A fresh cache each time: CMake keeps every FEATURE_* of an earlier
# configure, so a feature dropped from the list below would stay off.
Remove-Item -ErrorAction SilentlyContinue "$build/CMakeCache.txt"
Push-Location $build
try {
    $extra = @()
    $cmake = @()
    if ($version -ne "6.6.3") {
        $extra = "freetype", "timezone_locale", "hijricalendar", "islamiccivilcalendar", "jalalicalendar",
                 "sharedmemory", "systemsemaphore", "pdf", "textodfwriter", "textmarkdownreader",
                 "textmarkdownwriter", "sessionmanager", "socks5", "dnslookup", "networklistmanager",
                 "imageformat_ppm", "imageformat_xbm",
                 "quick-tableview", "quick-sprite", "quick-particles", "quick-designer",
                 "qml-xml-http-request", "qml-worker-script",
                 "quickcontrols2-fusion", "quickcontrols2-imagine", "quickcontrols2-material",
                 "quickcontrols2-universal", "quickcontrols2-fluentwinui3", "quickcontrols2-stylekit",
                 "quickcontrols2-windows" | ForEach-Object { "-no-feature-$_" }
        $cmake = "--", "-DQT_YAP_HLSL_ONLY=ON"
    }
    & "$src/configure.bat" -prefix $prefix -static -static-runtime -release -optimize-size -ltcg `
        -nomake examples -nomake tests -submodules ($modules -join ",") `
        -no-opengl -no-feature-vulkan -no-freetype -no-widgets -no-dbus -no-feature-sql -no-openssl -schannel `
        -no-feature-ffmpeg -qt-zlib -qt-libpng -qt-libjpeg -qt-harfbuzz -qt-pcre -qt-webp @extra @cmake
    if ($LASTEXITCODE) { throw "configure failed" }
    cmake --build . --parallel
    if ($LASTEXITCODE) { throw "build failed" }
    cmake --install .
    if ($LASTEXITCODE) { throw "install failed" }
} finally { Pop-Location }
"installed $prefix"
if ($Clean) { Remove-Item -Recurse -Force $build, "$Root/src"; "removed $build and $Root/src" }
