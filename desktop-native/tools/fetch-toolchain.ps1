# Downloads, checks and unpacks the native build toolchain: the static,
# patched Qt 6.11.3 and LiveKit's prebuilt libwebrtc, from the GitHub release
# that tools/package-toolchain.ps1 made. tools/toolchain.lock.json names the
# release and every asset's size and SHA-256; any mismatch stops here.
#
#   ./tools/fetch-toolchain.ps1 -Dest D:/toolchain                   # gh CLI, signed in
#   ./tools/fetch-toolchain.ps1 -Dest $env:RUNNER_TEMP/tc -DownloadDir $env:RUNNER_TEMP/tc-dl
#
# Afterwards <Dest>/qt is the Qt prefix (build.ps1 -QtDir, QMAKE =
# <Dest>/qt/bin/qmake.exe) and <Dest>/libwebrtc the libwebrtc folder
# (LK_CUSTOM_WEBRTC). In GitHub Actions the four variables are also written
# to GITHUB_ENV. docs/INSTALLER.md, "CI".
param(
    [Parameter(Mandatory)][string]$Dest,
    # Where the archives go; a cache can keep this folder. Archives already
    # there with the right checksum are not downloaded again.
    [string]$DownloadDir = (Join-Path $Dest "_download"),
    [string]$Lock = "$PSScriptRoot/toolchain.lock.json",
    [string]$Repo = $(if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { "115jon/yap" })
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$lockText = Get-Content -Raw $Lock
$toolchain = $lockText | ConvertFrom-Json
$lockHash = (Get-FileHash -InputStream ([IO.MemoryStream]::new([Text.Encoding]::UTF8.GetBytes($lockText)))).Hash
New-Item -ItemType Directory -Force $Dest, $DownloadDir | Out-Null
$Dest = (Resolve-Path $Dest).Path
$DownloadDir = (Resolve-Path $DownloadDir).Path

function Test-Asset($asset) {
    $path = Join-Path $DownloadDir $asset.name
    if (-not (Test-Path $path)) { return $false }
    if ((Get-Item $path).Length -ne [long]$asset.size) { return $false }
    return (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant() -eq $asset.sha256
}

$stamp = Join-Path $Dest ".toolchain-lock"
if ((Test-Path $stamp) -and ((Get-Content -Raw $stamp).Trim() -eq $lockHash) -and
    (Test-Path "$Dest/qt/bin/qmake.exe") -and (Test-Path "$Dest/libwebrtc/lib")) {
    Write-Host "==> toolchain $($toolchain.tag) already unpacked in $Dest"
} else {
    foreach ($asset in $toolchain.assets) {
        if (Test-Asset $asset) {
            Write-Host "==> $($asset.name): cached, checksum ok"
            continue
        }
        Write-Host "==> downloading $($asset.name) ($([math]::Round([long]$asset.size / 1MB)) MB)"
        gh release download $toolchain.tag --repo $Repo --pattern $asset.name --dir $DownloadDir --clobber
        if ($LASTEXITCODE) { throw "gh release download $($toolchain.tag) $($asset.name) failed ($LASTEXITCODE)" }
        if (-not (Test-Asset $asset)) { throw "$($asset.name) does not match tools/toolchain.lock.json (size or SHA-256)" }
        Write-Host "    checksum ok"
    }

    foreach ($dir in @("qt", "libwebrtc")) {
        if (Test-Path "$Dest/$dir") { Remove-Item -Recurse -Force "$Dest/$dir" }
    }
    $sevenZip = (Get-Command 7z -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1)
    $sevenZip = if ($sevenZip) { $sevenZip.Source } else { "$env:ProgramFiles\7-Zip\7z.exe" }
    if (-not (Test-Path $sevenZip)) { throw "7-Zip not found" }
    # One archive per component; a split one is opened at its first volume.
    foreach ($first in $toolchain.assets | Where-Object { $_.name -match '\.7z(\.001)?$' }) {
        Write-Host "==> unpacking $($first.name)"
        & $sevenZip x -y -bso0 -bsp0 "-o$Dest" (Join-Path $DownloadDir $first.name)
        if ($LASTEXITCODE) { throw "7z x $($first.name) failed ($LASTEXITCODE)" }
    }
    if (-not (Test-Path "$Dest/qt/bin/qmake.exe")) { throw "The Qt archive has no qt/bin/qmake.exe" }
    if (-not (Test-Path "$Dest/libwebrtc/lib")) { throw "The libwebrtc archive has no libwebrtc/lib" }
    Set-Content -NoNewline $stamp $lockHash
}

$qt = "$Dest/qt" -replace '\\', '/'
$webrtc = "$Dest/libwebrtc" -replace '\\', '/'
# qt.conf (Prefix=..) makes qmake report this folder, not the build PC's.
$prefix = (& "$qt/bin/qmake.exe" -query QT_INSTALL_PREFIX).Trim() -replace '\\', '/'
if ([IO.Path]::GetFullPath($prefix) -ne [IO.Path]::GetFullPath($qt)) {
    throw "qmake reports prefix $prefix, not $qt (bin/qt.conf missing?)"
}
Write-Host "==> Qt $(& "$qt/bin/qmake.exe" -query QT_VERSION) at $qt; libwebrtc at $webrtc"
if ($env:GITHUB_ENV) {
    @(
        "YAP_QT_DIR=$qt",
        "QMAKE=$qt/bin/qmake.exe",
        "YAP_LIBWEBRTC_DIR=$webrtc",
        "LK_CUSTOM_WEBRTC=$webrtc"
    ) | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}
