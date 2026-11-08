# Builds YapNative.exe: CMake + Ninja, MSVC from the newest Visual Studio,
# Qt from -QtDir (a static Qt gives one self-contained .exe).
param(
    # The size-optimized, patched static Qt 6.11.3 (patches/qt/build-qt.ps1);
    # qt-custom-6.11p8 carries patches 0001-0004 and 0006 (0006: Chromium's
    # text contrast and gamma, the text pass), qt-custom-6.11p7 0001-0005
    # (0005, retired: DirectWrite's enhanced contrast), qt-custom-6.11p6
    # 0001-0004, and
    # qt-custom-6.11 0001-0003. The stock static Qt 6.6.3 no longer builds the
    # app (QML needs 6.9+).
    [string]$QtDir = $(if ($env:YAP_QT_DIR) { $env:YAP_QT_DIR }
                       elseif (Test-Path "F:/ct/qt-custom-6.11p8/6.11.3-yap/bin/qmake.exe") { "F:/ct/qt-custom-6.11p8/6.11.3-yap" }
                       elseif (Test-Path "F:/ct/qt-custom-6.11p7/6.11.3-yap/bin/qmake.exe") { "F:/ct/qt-custom-6.11p7/6.11.3-yap" }
                       elseif (Test-Path "F:/ct/qt-custom-6.11p6/6.11.3-yap/bin/qmake.exe") { "F:/ct/qt-custom-6.11p6/6.11.3-yap" }
                       elseif (Test-Path "F:/ct/qt-custom-6.11/6.11.3-yap/bin/qmake.exe") { "F:/ct/qt-custom-6.11/6.11.3-yap" }
                       else { throw "No Qt 6.11.3: build it with patches/qt/build-qt.ps1, or pass -QtDir / set YAP_QT_DIR." }),
    [ValidateSet("Release", "RelWithDebInfo", "Debug")]
    [string]$Config = "Release",
    [string]$BuildDir = $(if ($env:YAP_NATIVE_BUILD_DIR) { $env:YAP_NATIVE_BUILD_DIR } else { "$PSScriptRoot/build/$Config" }),
    # ONNX Runtime 1.24.4 (its DirectML build), for camera backgrounds
    # (docs/LIBRARIES.md). Copied with the model into <BuildDir>/segmentation,
    # where the app looks; without it the background row says it cannot
    # apply one. DirectML itself is the system's (System32).
    [string]$OrtDir = $(if ($env:YAP_ORT_DIR) { $env:YAP_ORT_DIR } else { "F:/ct/onnxruntime/dml-1.24.4" })
)
$ErrorActionPreference = "Stop"

# LiveKit's prebuilt libwebrtc (static, /MT). Reuse an extracted copy instead
# of the ~430 MB download webrtc-sys does per target dir.
$webrtc = if ($env:YAP_LIBWEBRTC_DIR) { $env:YAP_LIBWEBRTC_DIR } else { "F:/ct/libwebrtc/win-x64-release-webrtc-a545de4/win-x64-release" }
if (-not $env:LK_CUSTOM_WEBRTC -and (Test-Path "$webrtc/lib")) { $env:LK_CUSTOM_WEBRTC = $webrtc }

# The patched crates rust/Cargo.toml builds (patches/vendor.ps1; a no-op
# when they are up to date).
& "$PSScriptRoot/patches/vendor.ps1"
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
# Visual Studio 2022, the MSVC patches/qt/build-qt.ps1 builds Qt with: Qt's
# libraries are link-time code (-ltcg), and so is our C++ (/GL), and LTCG
# cannot mix objects from two compiler versions.
$vs = & $vswhere -version "[17.0,18.0)" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
if (-not $vs) { throw "Visual Studio 2022 with the x64 C++ tools not found." }
Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null

# CMake keeps the Qt it first found (Qt6_DIR and each Qt6<Module>_DIR) in its
# cache and ignores a new prefix, so a build dir configured against another
# Qt is configured afresh.
$cache = Join-Path $BuildDir "CMakeCache.txt"
if (Test-Path $cache) {
    $cached = (Select-String -Path $cache -Pattern '^Qt6_DIR:PATH=(.*)$').Matches.Groups[1].Value
    $wanted = "$QtDir/lib/cmake/Qt6"
    if ($cached -and ([IO.Path]::GetFullPath($cached) -ne [IO.Path]::GetFullPath($wanted))) {
        Write-Host "Qt changed ($cached -> $wanted); configuring afresh."
        Remove-Item $cache
        Remove-Item -Recurse -Force (Join-Path $BuildDir "CMakeFiles") -ErrorAction SilentlyContinue
    }
}

cmake -S $PSScriptRoot -B $BuildDir -G Ninja "-DCMAKE_BUILD_TYPE=$Config" "-DCMAKE_PREFIX_PATH=$QtDir"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
cmake --build $BuildDir
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$segmentation = Join-Path $BuildDir "segmentation"
if (Test-Path "$OrtDir/onnxruntime.dll") {
    New-Item -ItemType Directory -Force $segmentation | Out-Null
    Copy-Item "$OrtDir/onnxruntime.dll" $segmentation
    Copy-Item "$PSScriptRoot/assets/models/selfie_segmenter.onnx" $segmentation
} else {
    Write-Warning "No ONNX Runtime in $OrtDir; camera backgrounds will be unavailable in this build."
}
exit 0
