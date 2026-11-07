# Builds the static Qt the native client links (docs/LIBRARIES.md, "Custom
# Qt"): Qt 6.6.3 from the qt-everywhere source zip, only the modules the
# app uses, optimized for size, with the patches in this folder.
#
#   pwsh build-qt.ps1 [-Zip <qt-everywhere-src-6.6.3.zip>] [-Root F:/ct/qt-custom]
#   pwsh build-qt.ps1 -Version 6.11.3 -Zip <...6.11.3.zip> -Root F:/ct/qt-custom-6.11
#
# Patches: *.patch here for 6.6.3, <Version>/*.patch for any other version.
#
# Output: <Root>/<Version>-yap (pass it to build.ps1 as -QtDir, or set
# YAP_QT_DIR). Intermediates: <Root>/src and <Root>/build, removed with
# -Clean once installed. Needs Visual Studio 2022 (the MSVC the stock
# static Qt was built with), CMake, Ninja and Python; ~25 GB while it
# builds, ~1.5 GB installed.
#
# Differences from the stock static Qt (F:/Files/Downloads/qt/src/6.6.3):
#   -optimize-size            /O1 for Qt itself (exe size; docs/PERFORMANCE.md)
#   -no-opengl -no-feature-vulkan   Qt Quick runs on D3D11 only
#   -no-freetype -no-widgets -no-dbus -no-feature-sql -no-openssl
#                             unused here (Schannel is the TLS backend)
#   -no-feature-ffmpeg        Qt Multimedia's Windows Media Foundation backend only
#   0001 (qtdeclarative)      QQuickWindow::releaseResources() also frees the
#                             batch renderer's pooled batches and glyph caches
#                             no text uses (the Settings memory, PERFORMANCE.md)
param(
    [string]$Version = "6.6.3",
    [string]$Zip = "F:/Files/Downloads/qt/src/qt-everywhere-src-$Version.zip",
    [string]$Root = "F:/ct/qt-custom",
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
Push-Location $build
try {
    & "$src/configure.bat" -prefix $prefix -static -static-runtime -release -optimize-size -ltcg `
        -nomake examples -nomake tests -submodules ($modules -join ",") `
        -no-opengl -no-feature-vulkan -no-freetype -no-widgets -no-dbus -no-feature-sql -no-openssl -schannel `
        -no-feature-ffmpeg -qt-zlib -qt-libpng -qt-libjpeg -qt-harfbuzz -qt-pcre -qt-webp
    if ($LASTEXITCODE) { throw "configure failed" }
    cmake --build . --parallel
    if ($LASTEXITCODE) { throw "build failed" }
    cmake --install .
    if ($LASTEXITCODE) { throw "install failed" }
} finally { Pop-Location }
"installed $prefix"
if ($Clean) { Remove-Item -Recurse -Force $build, "$Root/src"; "removed $build and $Root/src" }
