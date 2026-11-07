# Builds YapNative.exe (release) and packs it into a per-user Velopack
# installer: <OutDir>/<PackId>-win-Setup.exe, the full .nupkg and the
# release feed files. See docs/INSTALLER.md.
#
#   ./tools/package.ps1                          # build, pack, unsigned
#   ./tools/package.ps1 -Sign                    # Authenticode via Artifact Signing (az login)
#   ./tools/package.ps1 -SkipBuild -PackId YapNative.InstallTest -Version 0.0.1 -Title "Yap Native InstallTest"
#
# The version defaults to [workspace.package] in rust/Cargo.toml, the one
# CMakeLists.txt reads. vpk is a .NET global tool needed only here; the
# Setup.exe it makes is native, so nothing needs .NET at install time.
param(
    # Velopack's app id. Also the install folder (%LOCALAPPDATA%\<PackId>),
    # which uninstall deletes: never "YapNative", the app's data root.
    [string]$PackId = "YapNative.App",
    [string]$Version,
    [string]$Title = "Yap Native",
    [string]$Authors = "115jon",
    [string]$BuildDir = $(if ($env:YAP_NATIVE_BUILD_DIR) { $env:YAP_NATIVE_BUILD_DIR } else { "$PSScriptRoot/../build/Release" }),
    [string]$OutDir = "$PSScriptRoot/../build/installer",
    [string]$QtDir,
    # Use the exe already in -BuildDir.
    [switch]$SkipBuild,
    # Sign every binary in the package, Setup.exe included, through Azure
    # Artifact Signing (vpk --azureTrustedSignFile). Uses the Azure CLI
    # login (DefaultAzureCredential); CI logs in with OIDC first.
    [switch]$Sign,
    [string]$SigningEndpoint = $(if ($env:ARTIFACT_SIGNING_ENDPOINT) { $env:ARTIFACT_SIGNING_ENDPOINT } else { "https://eus.codesigning.azure.net/" }),
    [string]$SigningAccount = $(if ($env:ARTIFACT_SIGNING_ACCOUNT) { $env:ARTIFACT_SIGNING_ACCOUNT } else { "jon115signing" }),
    [string]$SigningProfile = $(if ($env:ARTIFACT_SIGNING_PROFILE) { $env:ARTIFACT_SIGNING_PROFILE } else { "publictrust" }),
    # The game capture hook's OBS-signed files (docs/INSTALLER.md): fetched
    # and verified by the desktop app's script, into a cache folder that is
    # reused across runs. -ObsZip uses an OBS release zip already on disk.
    [string]$ObsCacheDir = $(if ($env:YAP_OBS_CACHE_DIR) { $env:YAP_OBS_CACHE_DIR } else { Join-Path $env:TEMP "ralph-obs-signed-hook" }),
    [string]$ObsZip,
    # Leave the hook files out (the hook then always falls back to WGC).
    [switch]$NoCaptureHook,
    # Camera backgrounds (docs/LIBRARIES.md): ONNX Runtime 1.24.4's DirectML
    # build (Microsoft.ML.OnnxRuntime.DirectML), unpacked from its NuGet
    # package with its licence files. DirectML is the system's (Windows 10
    # 1903 and later ship it); where it is missing or too old, segmentation
    # runs on ONNX Runtime's CPU provider. The model comes from assets/models.
    [string]$OrtDir = $(if ($env:YAP_ORT_DIR) { $env:YAP_ORT_DIR } else { "F:/ct/onnxruntime/dml-1.24.4" }),
    # Leave the segmentation runtime out (backgrounds are then unavailable).
    [switch]$NoSegmentation
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = (Resolve-Path "$PSScriptRoot/..").Path
$cargoToml = Get-Content -Raw "$root/rust/Cargo.toml"

if (-not $Version) {
    # The first column-0 `version = "x.y.z"`: [workspace.package].
    $m = [regex]::Match($cargoToml, '(?m)^version = "(\d+\.\d+\.\d+)"')
    if (-not $m.Success) { throw "No workspace version in rust/Cargo.toml" }
    $Version = $m.Groups[1].Value
}
# vpk must match the velopack crate the exe links (rust/Cargo.toml).
$m = [regex]::Match($cargoToml, '(?m)^velopack = "=([^"]+)"')
if (-not $m.Success) { throw "No pinned velopack version in rust/Cargo.toml" }
$vpkVersion = $m.Groups[1].Value

# Velopack's vpk, never another tool of that name (a Python "vpk" exists).
function Find-Vpk {
    $candidates = @("$env:USERPROFILE\.dotnet\tools\vpk.exe")
    $onPath = Get-Command vpk -CommandType Application -ErrorAction SilentlyContinue
    if ($onPath) { $candidates += $onPath.Source }
    foreach ($c in $candidates) {
        if (-not (Test-Path $c)) { continue }
        $out = & $c --help --skip-updates 2>&1 | Out-String
        if ($out -match "Velopack CLI ([^\s,]+)") { return @{ Path = $c; Version = $Matches[1] } }
    }
    return $null
}
$vpk = Find-Vpk
if (-not $vpk -or $vpk.Version -ne $vpkVersion) {
    $dotnet = @("$env:USERPROFILE\.dotnet\dotnet.exe", "dotnet") |
        Where-Object { Get-Command $_ -ErrorAction SilentlyContinue } | Select-Object -First 1
    if (-not $dotnet) { throw "vpk $vpkVersion is missing and no dotnet SDK was found to install it." }
    $env:DOTNET_CLI_TELEMETRY_OPTOUT = "1"
    $verb = if ($vpk) { "update" } else { "install" }
    & $dotnet tool $verb -g vpk --version $vpkVersion
    if ($LASTEXITCODE) { throw "dotnet tool $verb vpk failed ($LASTEXITCODE)" }
    $vpk = Find-Vpk
    if (-not $vpk) { throw "vpk was installed but cannot be run." }
}
Write-Host "==> vpk $($vpk.Version), $PackId $Version"

if (-not $SkipBuild) {
    $buildArgs = @{ Config = "Release"; BuildDir = $BuildDir }
    if ($QtDir) { $buildArgs.QtDir = $QtDir }
    & "$root/build.ps1" @buildArgs
    if ($LASTEXITCODE) { throw "build.ps1 failed ($LASTEXITCODE)" }
}
$exe = Join-Path $BuildDir "YapNative.exe"
if (-not (Test-Path $exe)) { throw "No $exe; build first or drop -SkipBuild." }

# The package is exactly what is staged: the one static exe, the game
# capture hook's six OBS-signed files, and the camera background runtime.
New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
$stage = Join-Path $OutDir "stage-$PackId-$Version"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $exe $stage

# The OBS-signed capture hook, byte for byte as OBS released it, next to the
# exe (the app looks in obs-capture\obs-signed). The desktop app's fetch
# script downloads the pinned OBS zip if needed and checks every file against
# desktop/packaging/obs-signed-hook.lock.json: SHA-256, a valid Authenticode
# signature, the signer and its thumbprint. Any mismatch stops the build.
if (-not $NoCaptureHook) {
    $hookDir = Join-Path $stage "obs-capture\obs-signed"
    $fetch = Join-Path $root "..\desktop\scripts\fetch-obs-signed-hook.ps1"
    $fetchArgs = @{ Destination = $hookDir; CacheDir = $ObsCacheDir }
    if ($ObsZip) { $fetchArgs.ZipPath = $ObsZip }
    & $fetch @fetchArgs
    if (-not $?) { throw "fetch-obs-signed-hook.ps1 failed" }
    $lock = Get-Content -Raw (Join-Path $root "..\desktop\packaging\obs-signed-hook.lock.json") | ConvertFrom-Json
    foreach ($file in $lock.files) {
        $staged = Join-Path $hookDir $file.name
        $sha = (Get-FileHash -Algorithm SHA256 $staged).Hash.ToLowerInvariant()
        if ($sha -ne $file.sha256) { throw "Staged $($file.name) does not match the lock" }
    }
    Write-Host "==> OBS-signed capture hook $($lock.obs_version) staged and checked"
}

# Camera backgrounds: ONNX Runtime (Microsoft-signed, kept as shipped), the
# converted MediaPipe model, and the runtime's licences, in the
# segmentation folder (yap-segment loads them from there by full path).
if (-not $NoSegmentation) {
    $segDir = Join-Path $stage "segmentation"
    New-Item -ItemType Directory $segDir | Out-Null
    foreach ($file in @("onnxruntime.dll", "LICENSE-onnxruntime.txt", "ThirdPartyNotices-onnxruntime.txt")) {
        $src = Join-Path $OrtDir $file
        if (-not (Test-Path $src)) { throw "Missing $src (see -OrtDir, or pass -NoSegmentation)" }
        Copy-Item $src $segDir
    }
    Copy-Item "$root/assets/models/selfie_segmenter.onnx" $segDir
    foreach ($dll in @("onnxruntime.dll")) {
        $sig = Get-AuthenticodeSignature (Join-Path $segDir $dll)
        if ($sig.Status -ne "Valid" -or $sig.SignerCertificate.Subject -notmatch "Microsoft Corporation") {
            throw "$dll is not Microsoft-signed ($($sig.Status))"
        }
    }
    $segMb = [math]::Round(((Get-ChildItem $segDir | Measure-Object Length -Sum).Sum) / 1MB, 1)
    Write-Host "==> segmentation runtime staged ($segMb MB)"
}

$packArgs = @(
    "pack", "--skip-updates",
    "--packId", $PackId,
    "--packVersion", $Version,
    "--packTitle", $Title,
    "--packAuthors", $Authors,
    "--packDir", $stage,
    "--mainExe", "YapNative.exe",
    # vpk assumes x86 otherwise (Setup refuses nothing, but the feed and
    # the update stub would say so).
    "--runtime", "win-x64",
    "--icon", "$root/assets/app/yap.ico",
    "--splashImage", "$root/assets/app/yap-256.png",
    # The default profile's AUMID (cpp/desktopshell.cpp), so a pinned
    # shortcut and the running window group together.
    "--aumid", "com.115jon.yap.native",
    "--shortcuts", "Desktop,StartMenuRoot",
    "--outputDir", $OutDir
)

$metadata = $null
if ($Sign) {
    $metadata = Join-Path $OutDir "signing-metadata.json"
    @{
        Endpoint               = $SigningEndpoint
        CodeSigningAccountName = $SigningAccount
        CertificateProfileName = $SigningProfile
    } | ConvertTo-Json | Set-Content -Encoding utf8NoBOM $metadata
    $packArgs += @("--azureTrustedSignFile", $metadata)
    # Never re-sign OBS's files: a changed byte voids the OBS Project
    # signature that anti-cheat vendors allowlist.
    # Microsoft's ONNX Runtime keeps its own signature.
    $packArgs += @("--signExclude", "(obs-capture|segmentation)[\\/]")
}

& $vpk.Path @packArgs
$code = $LASTEXITCODE
Remove-Item -Recurse -Force $stage
if ($code) { throw "vpk pack failed ($code)" }

$setup = Join-Path $OutDir "$PackId-win-Setup.exe"
if (-not (Test-Path $setup)) { throw "vpk did not produce $setup" }
if ($Sign) {
    $sig = Get-AuthenticodeSignature $setup
    if ($sig.Status -ne "Valid") { throw "Setup.exe signature: $($sig.Status) $($sig.StatusMessage)" }
    Write-Host "==> signed by $($sig.SignerCertificate.Subject)"
}
Write-Host "==> $setup ($([math]::Round((Get-Item $setup).Length / 1MB, 1)) MB)"
