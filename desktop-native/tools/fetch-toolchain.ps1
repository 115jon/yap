# Downloads, checks and unpacks the native build toolchain: the static,
# patched Qt 6.11.3 and LiveKit's prebuilt libwebrtc, from the GitHub release
# that tools/package-toolchain.ps1 made. tools/toolchain.lock.json names the
# release and every asset's size and SHA-256; any mismatch stops here, and
# so does any unpacked file that differs from the archive's
# *.contents.sha256 list.
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
. "$PSScriptRoot/toolchain-files.ps1"

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
    # Every missing asset in one gh call, which downloads them side by side.
    $missing = @($toolchain.assets | Where-Object { -not (Test-Asset $_) })
    foreach ($asset in $toolchain.assets | Where-Object { $_ -notin $missing }) {
        Write-Host "==> $($asset.name): cached, checksum ok"
    }
    if ($missing) {
        Write-Host "==> downloading $(($missing | ForEach-Object { "$($_.name) ($([math]::Round([long]$_.size / 1MB)) MB)" }) -join ', ')"
        $patterns = $missing | ForEach-Object { "--pattern", $_.name }
        gh release download $toolchain.tag --repo $Repo @patterns --dir $DownloadDir --clobber
        if ($LASTEXITCODE) { throw "gh release download $($toolchain.tag) failed ($LASTEXITCODE)" }
        foreach ($asset in $missing) {
            if (-not (Test-Asset $asset)) { throw "$($asset.name) does not match tools/toolchain.lock.json (size or SHA-256)" }
        }
        Write-Host "    checksums ok"
    }

    # The contents lists (*.contents.sha256: "<sha256>  <component>/<path>").
    # An empty library or object never links (LNK1136), so a list with one
    # is refused before anything is unpacked.
    $want = @{}
    foreach ($list in $toolchain.assets | Where-Object { $_.name -like '*.contents.sha256' }) {
        foreach ($line in [IO.File]::ReadAllLines((Join-Path $DownloadDir $list.name))) {
            if ($line -notmatch '^([0-9a-f]{64})  ([^/]+/.+)$') { throw "$($list.name): unreadable line '$line'" }
            $want[$Matches[2]] = $Matches[1]
        }
    }
    if (-not $want.Count) { throw "tools/toolchain.lock.json lists no *.contents.sha256" }
    $empty = @($want.Keys | Where-Object { $want[$_] -eq $EmptySha256 -and $_ -match '\.(lib|obj)$' } | Sort-Object)
    if ($empty) { throw "The toolchain packs $($empty.Count) empty libraries or objects, which do not link:`n  $(($empty | Select-Object -First 20) -join "`n  ")" }
    $components = @($want.Keys | ForEach-Object { $_.Split('/')[0] } | Sort-Object -Unique)

    foreach ($dir in @("qt", "libwebrtc")) {
        if (Test-Path "$Dest/$dir") { Remove-Item -Recurse -Force "$Dest/$dir" }
    }
    $sevenZip = (Get-Command 7z -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1)
    $sevenZip = if ($sevenZip) { $sevenZip.Source } else { "$env:ProgramFiles\7-Zip\7z.exe" }
    if (-not (Test-Path $sevenZip)) { throw "7-Zip not found" }
    # One archive per component, unpacked side by side (each 7z is mostly
    # writing small files); a split one is opened at its first volume.
    $unpack = foreach ($first in $toolchain.assets | Where-Object { $_.name -match '\.7z(\.001)?$' }) {
        Write-Host "==> unpacking $($first.name)"
        $p = Start-Process -FilePath $sevenZip -NoNewWindow -PassThru -ArgumentList @(
            "x", "-y", "-bso0", "-bsp0", "`"-o$Dest`"", "`"$(Join-Path $DownloadDir $first.name)`"")
        $null = $p.Handle  # keeps the exit code readable after it exits
        [pscustomobject]@{ Name = $first.name; Process = $p }
    }
    foreach ($u in $unpack) {
        $u.Process.WaitForExit()
        if ($u.Process.ExitCode) { throw "7z x $($u.Name) failed ($($u.Process.ExitCode))" }
    }
    if (-not (Test-Path "$Dest/qt/bin/qmake.exe")) { throw "The Qt archive has no qt/bin/qmake.exe" }
    if (-not (Test-Path "$Dest/libwebrtc/lib")) { throw "The libwebrtc archive has no libwebrtc/lib" }

    # Every unpacked file against its archive's contents list (hashed in
    # parallel): a missing, extra, emptied or changed file stops here, not
    # at the link.
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $got = @(foreach ($dir in $components) {
        $root = (Resolve-Path "$Dest/$dir").Path
        Get-ChildItem -Recurse -File -Force $root | ForEach-Object {
            [pscustomobject]@{ Rel = "$dir/" + ($_.FullName.Substring($root.Length + 1) -replace '\\', '/'); Path = $_.FullName; Length = $_.Length }
        }
    })
    $digests = Get-FileDigests $got
    $bad = [Collections.Generic.List[string]]::new()
    foreach ($rel in $want.Keys) {
        if (-not $digests.ContainsKey($rel)) { $bad.Add("missing: $rel") }
        elseif ($digests[$rel].Hash -ne $want[$rel]) { $bad.Add("SHA-256 differs ($($digests[$rel].Length) bytes): $rel") }
    }
    foreach ($rel in $digests.Keys) { if (-not $want.ContainsKey($rel)) { $bad.Add("not in the contents list: $rel") } }
    if ($bad.Count) { throw "The unpacked toolchain does not match its contents lists ($($bad.Count)):`n  $(($bad | Sort-Object | Select-Object -First 40) -join "`n  ")" }
    $gb = [math]::Round((($got | Measure-Object Length -Sum).Sum) / 1GB, 1)
    Write-Host "==> contents ok: $($digests.Count) files, $gb GB, SHA-256 in $([math]::Round($watch.Elapsed.TotalSeconds, 1)) s"
    Set-Content -LiteralPath $stamp -Value $lockHash -NoNewline
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
