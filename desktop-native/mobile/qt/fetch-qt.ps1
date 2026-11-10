# Downloads, checks and unpacks the Qt 6.11 packages the Qt mobile app
# builds with, from download.qt.io's online repository (the archives the Qt
# online installer uses). qt-sdk.lock.json names every archive's URL and
# SHA-256; a mismatch stops here.
#
#   pwsh desktop-native/mobile/qt/fetch-qt.ps1 -Dest <dir> -Set android-arm64_v8a,host-linux
#   pwsh desktop-native/mobile/qt/fetch-qt.ps1 -Refresh      # re-pin from Updates.xml
#
# Each set unpacks into <Dest>/<its dir> (the lock's "dir": android_x86_64,
# android_arm64_v8a, gcc_64, macos, ios), a Qt prefix of its own. Archives
# are kept in -DownloadDir (default <Dest>/_download) and not downloaded
# again while their checksum matches.
#
# -Refresh reads each set's Updates.xml, picks the lock's packages and, from
# their archives, those of the lock's modules, and records each archive's
# URL and the SHA-256 that download.qt.io publishes next to it (<url>.sha256;
# the archives themselves come from mirrors). Nothing is downloaded but the
# Updates.xml and .sha256 files.
#
# A target Qt (Android, iOS) still names the Qt build machine's host Qt in
# bin/qmake and bin/target_qt.conf; build.sh points them at QT_HOST_PATH.
# Runs on Windows, Linux and macOS (PowerShell 7, 7-Zip as 7z, 7zz or 7za).
param(
    [string]$Dest,
    [string[]]$Set,
    [string]$DownloadDir,
    [string]$Lock = "$PSScriptRoot/qt-sdk.lock.json",
    [switch]$Refresh
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$ProgressPreference = "SilentlyContinue"  # Invoke-WebRequest's progress bar is slow

$lockData = Get-Content -Raw $Lock | ConvertFrom-Json

if ($Refresh) {
    foreach ($prop in $lockData.sets.PSObject.Properties) {
        $s = $prop.Value
        $base = "$($lockData.repository)/$($s.path)"
        Write-Host "==> $($prop.Name): $base/Updates.xml"
        [xml]$updates = (Invoke-WebRequest -Uri "$base/Updates.xml").Content
        $archives = [Collections.Generic.List[object]]::new()
        foreach ($name in $s.packages) {
            $pkg = @($updates.Updates.PackageUpdate | Where-Object { $_.Name -eq $name })
            if ($pkg.Count -ne 1) { throw "$($prop.Name): no package $name in $base/Updates.xml" }
            $pkg = $pkg[0]
            if ($pkg.Version -notlike "$($lockData.qt)-*") { throw "$name is $($pkg.Version), not Qt $($lockData.qt)" }
            $files = @($pkg.DownloadableArchives -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
            foreach ($file in $files) {
                $module = ($file -split '-')[0]
                if ($module -notin $lockData.modules) { continue }
                $url = "$base/$name/$($pkg.Version)$file"
                $sha = ((Invoke-WebRequest -Uri "$url.sha256").Content.ToString().Trim() -split '\s+')[0].ToLowerInvariant()
                if ($sha -notmatch '^[0-9a-f]{64}$') { throw "$url.sha256: unreadable" }
                Write-Host "    $module  $sha"
                $entry = [ordered]@{ url = $url; sha256 = $sha }
                # Most archives unpack at the prefix; some (ICU on Linux)
                # into a folder under it, as the package's Extract says.
                $extract = @($pkg.Operations.Operation | Where-Object { $_.name -eq 'Extract' -and $_.Argument[1] -eq $file })
                if ($extract -and $extract[0].Argument[0] -match '^@TargetDir@/[^/]+/[^/]+/(.+)$') { $entry.subdir = $Matches[1] }
                $archives.Add($entry)
            }
        }
        if (-not $archives.Count) { throw "$($prop.Name): no archives of the lock's modules" }
        $s.archives = @($archives)
    }
    $lockData | ConvertTo-Json -Depth 8 | ForEach-Object { $_ -replace "`r`n", "`n" } | Set-Content -NoNewline -LiteralPath $Lock
    Add-Content -LiteralPath $Lock -Value "`n" -NoNewline
    Write-Host "==> $Lock updated"
    return
}

if (-not $Dest) { throw "-Dest is required (or -Refresh)" }
if (-not $Set) { throw "-Set is required: $(($lockData.sets.PSObject.Properties.Name) -join ', ')" }
$Set = @($Set | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if (-not $DownloadDir) { $DownloadDir = Join-Path $Dest "_download" }
New-Item -ItemType Directory -Force $Dest, $DownloadDir | Out-Null
$Dest = (Resolve-Path $Dest).Path
$DownloadDir = (Resolve-Path $DownloadDir).Path

$sevenZip = foreach ($name in "7z", "7zz", "7za") {
    $cmd = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cmd) { $cmd.Source; break }
}
if (-not $sevenZip -and $IsWindows -and (Test-Path "$env:ProgramFiles\7-Zip\7z.exe")) { $sevenZip = "$env:ProgramFiles\7-Zip\7z.exe" }
if (-not $sevenZip) { throw "7-Zip not found (7z, 7zz or 7za on PATH)" }
# Qt's Linux and macOS archives chain symbolic links (libQt6DBus.so ->
# libQt6DBus.so.6 -> ...), which 7-Zip 25 and later refuse as "dangerous
# links" unless told otherwise (-snld20). Older 7-Zip and p7zip restore
# them without the switch, and do not know it.
$sevenZipArgs = @()
$banner = (& $sevenZip 2>&1 | Select-Object -First 4) -join "`n"
if ($banner -match '7-Zip(?: \(\w\))? (\d+)\.' -and [int]$Matches[1] -ge 25) { $sevenZipArgs += "-snld20" }
$version = ($banner -split "`n" | Where-Object { $_ -match '7-Zip' } | Select-Object -First 1)
Write-Host "==> $sevenZip $(if ($version) { "[$($version.Trim())]" }) $sevenZipArgs"

function Get-Sha256($path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() }

foreach ($name in $Set) {
    $s = $lockData.sets.PSObject.Properties[$name]
    if (-not $s) { throw "No set '$name' in $Lock ($(($lockData.sets.PSObject.Properties.Name) -join ', '))" }
    $s = $s.Value
    $prefix = Join-Path $Dest $s.dir
    # The stamp is the set's archive list: a re-pinned lock unpacks afresh.
    $want = ($s.archives | ForEach-Object { "$($_.sha256) $($_.url) $($_.PSObject.Properties['subdir'].Value)" }) -join "`n"
    $stamp = Join-Path $prefix ".qt-sdk-lock"
    if ((Test-Path $stamp) -and ((Get-Content -Raw $stamp) -eq $want)) {
        Write-Host "==> $name already unpacked in $prefix"
        continue
    }
    if (Test-Path $prefix) { Remove-Item -Recurse -Force $prefix }
    New-Item -ItemType Directory -Force $prefix | Out-Null
    foreach ($a in $s.archives) {
        $file = Join-Path $DownloadDir ([Uri]$a.url).Segments[-1]
        if ((Test-Path $file) -and (Get-Sha256 $file) -eq $a.sha256) {
            Write-Host "==> $(Split-Path -Leaf $file): cached, checksum ok"
        } else {
            Write-Host "==> downloading $($a.url)"
            for ($try = 1; ; $try++) {
                try { Invoke-WebRequest -Uri $a.url -OutFile $file -MaximumRedirection 10; break }
                catch { if ($try -ge 3) { throw }; Write-Host "    retry $try ($($_.Exception.Message))"; Start-Sleep -Seconds (5 * $try) }
            }
            $got = Get-Sha256 $file
            if ($got -ne $a.sha256) {
                Remove-Item -Force $file
                throw "$($a.url): SHA-256 $got, the lock says $($a.sha256)"
            }
        }
        $into = $prefix
        if ($a.PSObject.Properties['subdir']) { $into = Join-Path $prefix $a.subdir }
        & $sevenZip x -y -bso0 -bsp0 @sevenZipArgs "-o$into" $file
        if ($LASTEXITCODE) { throw "7z x $file failed ($LASTEXITCODE)" }
    }
    if (-not (Test-Path (Join-Path $prefix "lib/cmake/Qt6/Qt6Config.cmake"))) { throw "$name unpacked without lib/cmake/Qt6" }
    Set-Content -LiteralPath $stamp -Value $want -NoNewline
    Write-Host "==> $name in $prefix"
}
