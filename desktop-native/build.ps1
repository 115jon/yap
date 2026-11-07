# Builds YapNative.exe: CMake + Ninja, MSVC from the newest Visual Studio,
# Qt from -QtDir (a static Qt gives one self-contained .exe).
param(
    # The size-optimized, patched static Qt (patches/qt/build-qt.ps1) when
    # it is there; the stock static Qt otherwise.
    [string]$QtDir = $(if ($env:YAP_QT_DIR) { $env:YAP_QT_DIR }
                       elseif (Test-Path "F:/ct/qt-custom/6.6.3-yap/bin/qmake.exe") { "F:/ct/qt-custom/6.6.3-yap" }
                       else { "F:/Files/Downloads/qt/src/6.6.3" }),
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
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "No Visual Studio with the x64 C++ tools found." }
Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null

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
