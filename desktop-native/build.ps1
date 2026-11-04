# Builds YapNative.exe: CMake + Ninja, MSVC from the newest Visual Studio,
# Qt from -QtDir (a static Qt gives one self-contained .exe).
param(
    [string]$QtDir = $(if ($env:YAP_QT_DIR) { $env:YAP_QT_DIR } else { "F:/Files/Downloads/qt/src/6.6.3" }),
    [ValidateSet("Release", "RelWithDebInfo", "Debug")]
    [string]$Config = "Release",
    [string]$BuildDir = $(if ($env:YAP_NATIVE_BUILD_DIR) { $env:YAP_NATIVE_BUILD_DIR } else { "$PSScriptRoot/build/$Config" })
)
$ErrorActionPreference = "Stop"

# LiveKit's prebuilt libwebrtc (static, /MT). Reuse an extracted copy instead
# of the ~430 MB download webrtc-sys does per target dir.
$webrtc = if ($env:YAP_LIBWEBRTC_DIR) { $env:YAP_LIBWEBRTC_DIR } else { "F:/ct/libwebrtc/win-x64-release-webrtc-a545de4/win-x64-release" }
if (-not $env:LK_CUSTOM_WEBRTC -and (Test-Path "$webrtc/lib")) { $env:LK_CUSTOM_WEBRTC = $webrtc }

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "No Visual Studio with the x64 C++ tools found." }
Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null

cmake -S $PSScriptRoot -B $BuildDir -G Ninja "-DCMAKE_BUILD_TYPE=$Config" "-DCMAKE_PREFIX_PATH=$QtDir"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
cmake --build $BuildDir
exit $LASTEXITCODE
