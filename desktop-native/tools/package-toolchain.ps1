# Packs the native build toolchain for CI: the static, patched Qt 6.11.3
# (patches/qt/build-qt.ps1) and LiveKit's prebuilt libwebrtc (the one
# webrtc-sys 0.3.52 downloads, webrtc-a545de4), as 7-Zip archives in
# volumes under 2 GB, and writes tools/toolchain.lock.json (names, sizes,
# SHA-256) for tools/fetch-toolchain.ps1. -Upload puts them on the
# toolchain's GitHub release (a pre-release, never "latest").
#
#   ./tools/package-toolchain.ps1                    # pack into -OutDir, write the lock
#   ./tools/package-toolchain.ps1 -Upload            # and create/update the release
#   ./tools/package-toolchain.ps1 -UploadOnly        # upload what an earlier run packed
#
# What is left out of Qt is in tools/toolchain-qt.exclude (docs, SBOMs and
# PDBs); every other file is packed as it is, except bin/qt.conf (added) and
# the qt-cmake*.bat scripts (the build PC's cmake path taken out).
# Each archive's file list and per-file SHA-256 go to <name>.contents.sha256:
# packing the same toolchain again gives the same contents files, and, with
# the same 7-Zip version, the same archive bytes (no access or creation
# times are stored; files are added in sorted order).
#
# Before the lock is written, each archive is listed (7z l -slt) and
# unpacked into a scratch folder, and every file's size and SHA-256 is
# compared with the source: any missing, extra, emptied or changed file
# stops here. So does a packed file that holds this PC's user profile path.
param(
    [string]$QtDir = "F:/ct/qt-custom-6.11p8/6.11.3-yap",
    [string]$WebrtcDir = "F:/ct/libwebrtc/win-x64-release-webrtc-a545de4/win-x64-release",
    [string]$OutDir = "F:/ct/yapn/toolchain",
    # -r2: the first release (no suffix) packed 176 Qt libraries and objects
    # as empty stubs, and a build that linked one failed with LNK1136.
    [string]$Tag = "native-toolchain-qt6.11.3-yap-webrtc-a545de4-r2",
    # The Qt build's patch set (build.ps1's comment): p8 = 0001-0004 and 0006.
    [string]$QtBuild = "qt-custom-6.11p8",
    [string]$WebrtcBuild = "webrtc-a545de4",
    [string]$Exclude = "$PSScriptRoot/toolchain-qt.exclude",
    [string]$Lock = "$PSScriptRoot/toolchain.lock.json",
    [int]$VolumeMB = 1900,
    [string]$Repo = "115jon/yap",
    [switch]$Upload,
    # Upload the archives already in -OutDir (they must match the lock).
    [switch]$UploadOnly
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. "$PSScriptRoot/toolchain-files.ps1"

$sevenZip = (Get-Command 7z -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1)
$sevenZip = if ($sevenZip) { $sevenZip.Source } else { "$env:ProgramFiles\7-Zip\7z.exe" }
$sevenZipVersion = [regex]::Match((& $sevenZip i | Out-String), '7-Zip (\S+)').Groups[1].Value

# The MSVC the Qt libraries were compiled with: their LTCG objects link
# only with that compiler (the CI job warns when its MSVC differs).
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -version "[17.0,18.0)" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
$msvc = (Get-Content "$vs\VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt").Trim()

# Glob (relative, "/", "*" and "**") to regex.
function ConvertTo-Regex([string]$glob) {
    $re = [regex]::Escape($glob) -replace '\\\*\\\*/', '(.*/)?' -replace '\\\*\\\*', '.*' -replace '\\\*', '[^/]*' -replace '\\\?', '[^/]'
    return "^$re$"
}
$drops = @()
foreach ($line in Get-Content $Exclude) {
    $line = $line.Trim()
    if (-not $line -or $line.StartsWith("#")) { continue }
    $kind, $glob = $line -split '\s+', 2
    switch ($kind) {
        "drop" { $drops += ConvertTo-Regex $glob }
        default { throw "toolchain-qt.exclude: unknown rule '$line'" }
    }
}

# Every file under $root as Rel (with "/"), Path and Length, sorted; dropped
# ones left out.
function Get-Files([string]$root, [string[]]$skip) {
    $full = (Resolve-Path $root).Path
    return @(Get-ChildItem -Recurse -File -Force $full | ForEach-Object {
        $rel = $_.FullName.Substring($full.Length + 1) -replace '\\', '/'
        $out = $false
        foreach ($re in $skip) { if ($rel -match $re) { $out = $true; break } }
        if (-not $out) { [pscustomobject]@{ Rel = $rel; Path = $_.FullName; Length = $_.Length } }
    } | Sort-Object Rel)
}

# This PC's profile path, as build files spell it: no packed file may hold
# it (the release is public).
$userProfile = [IO.Path]::GetFullPath($env:USERPROFILE).TrimEnd('\')
$leakNeedles = @($userProfile, ($userProfile -replace '\\', '/'), ($userProfile -replace '\\', '\\'))

# Lists $archive (7z l -slt) and unpacks it into $scratch, then compares
# every file with $expected (Rel "<component>/..." -> Length, Hash). Throws
# on any missing, extra, emptied or changed file.
function Test-Archive([string]$archive, [hashtable]$expected, [string]$scratch) {
    $bad = [Collections.Generic.List[string]]::new()
    $listed = @{}
    $entry = $null
    $body = $false
    $listing = & $sevenZip l -slt $archive
    if ($LASTEXITCODE) { throw "7z l $archive failed ($LASTEXITCODE)" }
    foreach ($line in $listing) {
        if (-not $body) { if ($line -eq "----------") { $body = $true }; continue }
        if ($line -match '^Path = (.*)$') { $entry = @{ Rel = $Matches[1] -replace '\\', '/'; Size = [long]0 } }
        elseif ($entry -and $line -match '^Size = (\d+)$') { $entry.Size = [long]$Matches[1] }
        elseif ($entry -and $line -match '^Attributes = (.*)$') {
            if (-not $Matches[1].StartsWith("D")) { $listed[$entry.Rel] = $entry.Size }
            $entry = $null
        }
    }
    foreach ($rel in $expected.Keys) {
        if (-not $listed.ContainsKey($rel)) { $bad.Add("not in the archive: $rel") }
        elseif ($listed[$rel] -ne $expected[$rel].Length) { $bad.Add("listed as $($listed[$rel]) bytes, the source has $($expected[$rel].Length): $rel") }
    }
    foreach ($rel in $listed.Keys) { if (-not $expected.ContainsKey($rel)) { $bad.Add("not in the source: $rel") } }
    if ($bad.Count) { throw "$archive does not match the source ($($bad.Count)):`n  $(($bad | Select-Object -First 40) -join "`n  ")" }
    Write-Host "    listing: $($listed.Count) files, sizes match the source"

    if (Test-Path $scratch) { Remove-Item -Recurse -Force $scratch }
    try {
        & $sevenZip x -y -bso0 -bsp0 "-o$scratch" $archive
        if ($LASTEXITCODE) { throw "7z x $archive failed ($LASTEXITCODE)" }
        $root = (Resolve-Path $scratch).Path
        $got = @(Get-ChildItem -Recurse -File -Force $root | ForEach-Object {
            [pscustomobject]@{ Rel = $_.FullName.Substring($root.Length + 1) -replace '\\', '/'; Path = $_.FullName; Length = $_.Length }
        })
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $digests = Get-FileDigests $got
        foreach ($rel in $expected.Keys) {
            $want = $expected[$rel]
            $have = $digests[$rel]
            if (-not $have) { $bad.Add("not unpacked: $rel") }
            elseif ($have.Length -eq 0 -and $want.Length -ne 0) { $bad.Add("unpacked empty, the source has $($want.Length) bytes: $rel") }
            elseif ($have.Length -ne $want.Length -or $have.Hash -ne $want.Hash) { $bad.Add("unpacked $($have.Length) bytes $($have.Hash), the source $($want.Length) bytes $($want.Hash): $rel") }
        }
        foreach ($rel in $digests.Keys) { if (-not $expected.ContainsKey($rel)) { $bad.Add("unpacked but not in the source: $rel") } }
        if ($bad.Count) { throw "$archive unpacks differently from the source ($($bad.Count)):`n  $(($bad | Select-Object -First 40) -join "`n  ")" }
        Write-Host "    unpacked: $($digests.Count) files, sizes and SHA-256 match the source ($([math]::Round($watch.Elapsed.TotalSeconds)) s to hash)"
    } finally {
        if (Test-Path $scratch) { Remove-Item -Recurse -Force $scratch }
    }
}

New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

$names = @{
    qt        = "qt-6.11.3-yap-p8-msvc-x64-static.7z"
    libwebrtc = "libwebrtc-$WebrtcBuild-win-x64-release.7z"
}

if (-not $UploadOnly) {
    $stage = Join-Path $OutDir "stage"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }

    # Stage each component under its folder name (hard links when on the same
    # volume, so staging 7 GB costs nothing), then archive the folder.
    function New-Stage([string]$name, $files) {
        $dir = Join-Path $stage $name
        foreach ($f in $files) {
            $dest = Join-Path $dir ($f.Rel -replace '/', '\')
            New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
            try { New-Item -ItemType HardLink -Path $dest -Target $f.Path | Out-Null }
            catch { Copy-Item -LiteralPath $f.Path $dest }
        }
        return $dir
    }
    # Gives a staged file new text. The hard link goes first, so the source
    # file stays as it is.
    function Set-StagedText([string]$dir, [string]$rel, [string]$text) {
        $path = Join-Path $dir ($rel -replace '/', '\')
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        [IO.File]::WriteAllText($path, $text, [Text.Encoding]::ASCII)
        return $rel
    }

    $components = [ordered]@{}
    $sources = [ordered]@{}
    $qtFiles = @(Get-Files $QtDir $drops)
    $components["qt"] = New-Stage "qt" $qtFiles
    # A static Qt is not relocatable: qmake reports the build PC's prefix
    # unless a qt.conf beside it says otherwise (cxx-qt asks qmake for paths).
    $changed = @(Set-StagedText $components["qt"] "bin/qt.conf" "[Paths]`r`nPrefix = ..`r`n")
    # qt-cmake*.bat name the build PC's cmake.exe (under its user profile)
    # and fall back to the cmake on PATH: packed with the fallback only.
    foreach ($f in $qtFiles | Where-Object Rel -match '^bin/qt-cmake[^/]*\.bat$') {
        $text = [IO.File]::ReadAllText($f.Path) -replace '(?m)^set cmake_path=[^\r\n]*', 'set cmake_path=cmake'
        $changed += Set-StagedText $components["qt"] $f.Rel $text
    }
    # What each archive must hold: the source files, and the staged copies
    # of the ones changed above.
    $sources["qt"] = @($qtFiles | Where-Object Rel -notin $changed) + @($changed | ForEach-Object {
        $path = Join-Path $components["qt"] ($_ -replace '/', '\')
        [pscustomobject]@{ Rel = $_; Path = $path; Length = (Get-Item -LiteralPath $path).Length }
    })
    $webrtcFiles = @(Get-Files $WebrtcDir @())
    $components["libwebrtc"] = New-Stage "libwebrtc" $webrtcFiles
    $sources["libwebrtc"] = $webrtcFiles

    Get-ChildItem $OutDir -File | Where-Object { $_.Name -match '\.7z(\.\d+)?$|\.contents\.sha256$' } | Remove-Item
    $assets = @()
    foreach ($name in $components.Keys) {
        $dir = $components[$name]
        $archive = Join-Path $OutDir $names[$name]

        # The contents list: every source file's SHA-256 and path, sorted.
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $digests = Get-FileDigests @($sources[$name] | ForEach-Object {
            [pscustomobject]@{ Rel = "$name/$($_.Rel)"; Path = $_.Path; Length = $_.Length }
        }) $leakNeedles
        $leaks = @($digests.Values | Where-Object Leak | ForEach-Object Rel | Sort-Object)
        if ($leaks) { throw "These files hold $userProfile (the release is public):`n  $($leaks -join "`n  ")" }
        $staged = @(Get-ChildItem -Recurse -File -Force $dir).Count
        if ($staged -ne $digests.Count) { throw "$name`: $staged files staged, $($digests.Count) in the source" }
        $rows = @($digests.Values | Sort-Object Rel)
        Write-Host "    hashed the source in $([math]::Round($watch.Elapsed.TotalSeconds)) s; no file holds $userProfile"
        $contents = "$archive.contents.sha256"
        ($rows | ForEach-Object { "$($_.Hash)  $($_.Rel)" }) -join "`n" | Set-Content -NoNewline -Encoding utf8NoBOM $contents
        $mb = [math]::Round((($rows | Measure-Object Length -Sum).Sum) / 1MB)
        Write-Host "==> $name`: $($rows.Count) files, $mb MB unpacked"

        $list = Join-Path $OutDir "$name.list"
        ($rows | ForEach-Object { $_.Rel }) -join "`n" | Set-Content -NoNewline -Encoding utf8NoBOM $list
        Push-Location $stage
        try {
            & $sevenZip a -t7z -m0=lzma2 -mx=7 -mmt=on -mqs=off -mtc=off -mta=off -mtm=on -bso0 -bsp0 "-v${VolumeMB}m" $archive "@$list"
            if ($LASTEXITCODE) { throw "7z a $archive failed ($LASTEXITCODE)" }
        } finally { Pop-Location }
        Remove-Item $list
        $first = Get-ChildItem $OutDir -File -Filter "$($names[$name]).*" | Where-Object Name -match '\.\d{3}$' | Sort-Object Name | Select-Object -First 1
        Test-Archive $first.FullName $digests (Join-Path $OutDir "verify-$name")
        foreach ($volume in Get-ChildItem $OutDir -File -Filter "$($names[$name]).*" | Where-Object Name -match '\.\d{3}$' | Sort-Object Name) {
            $assets += [ordered]@{
                name   = $volume.Name
                size   = $volume.Length
                sha256 = (Get-FileHash -Algorithm SHA256 $volume.FullName).Hash.ToLowerInvariant()
            }
            Write-Host "    $($volume.Name): $([math]::Round($volume.Length / 1MB)) MB"
        }
        $assets += [ordered]@{
            name   = (Split-Path -Leaf $contents)
            size   = (Get-Item $contents).Length
            sha256 = (Get-FileHash -Algorithm SHA256 $contents).Hash.ToLowerInvariant()
        }
    }
    Remove-Item -Recurse -Force $stage

    $qtVersion = (& "$QtDir/bin/qmake.exe" -query QT_VERSION).Trim()
    $lockData = [ordered]@{
        tag       = $Tag
        msvc      = $msvc
        qt        = [ordered]@{
            version = $qtVersion
            build   = $QtBuild
            script  = "desktop-native/patches/qt/build-qt.ps1"
            patches = "desktop-native/patches/qt/$qtVersion"
            files   = $sources["qt"].Count
        }
        libwebrtc = [ordered]@{
            build  = $WebrtcBuild
            source = "LiveKit's prebuilt libwebrtc (webrtc-sys 0.3.52: github.com/livekit/rust-sdks/releases/download/webrtc-a545de4/webrtc-win-x64-release.zip), unmodified"
            files  = $webrtcFiles.Count
        }
        sevenZip  = $sevenZipVersion
        assets    = $assets
    }
    ($lockData | ConvertTo-Json -Depth 5) -replace "`r`n", "`n" | Set-Content -NoNewline -Encoding utf8NoBOM $Lock
    Add-Content -NoNewline -Encoding utf8NoBOM $Lock "`n"
    Write-Host "==> wrote $Lock"
} else {
    # The archives from an earlier run, checked against the lock.
    $assets = @((Get-Content -Raw $Lock | ConvertFrom-Json).assets)
    foreach ($a in $assets) {
        $path = Join-Path $OutDir $a.name
        if (-not (Test-Path $path) -or (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant() -ne $a.sha256) {
            throw "$path is missing or does not match the lock; pack again"
        }
    }
    $qtVersion = (& "$QtDir/bin/qmake.exe" -query QT_VERSION).Trim()
}

if ($Upload -or $UploadOnly) {
    $body = @"
Build toolchain for Yap's native Windows app (``desktop-native/``), for CI only. Not an app release: the app's releases are tagged ``desktop-native-v*``.

- **Qt $qtVersion, static, patched** (``$($names.qt).*``): built with ``desktop-native/patches/qt/build-qt.ps1`` from the official ``qt-everywhere-src-$qtVersion`` source (https://download.qt.io/official_releases/qt/), modules qtbase, qtshadertools, qtdeclarative, qtsvg, qtimageformats and qtmultimedia, ``-static -static-runtime -release -optimize-size -ltcg``, MSVC $msvc, with the patches in ``desktop-native/patches/qt/$qtVersion/``. Docs, SBOMs and PDBs dropped (``desktop-native/tools/toolchain-qt.exclude``); every library and object is packed whole. ``bin/qt.conf`` added so the prefix is relocatable, and the ``bin/qt-cmake*.bat`` scripts use the ``cmake`` on PATH.
- **libwebrtc $WebrtcBuild** (``$($names.libwebrtc).*``): LiveKit's prebuilt ``webrtc-win-x64-release`` (the one ``webrtc-sys`` 0.3.52 downloads), unmodified. Yap's changes to the Rust bindings are source patches in ``desktop-native/patches/webrtc-sys`` and ``desktop-native/patches/libwebrtc``, applied by ``desktop-native/patches/vendor.ps1``.

**Licences.** Qt is used under the GNU LGPL v3 (and GPL v3 for its tools); the corresponding source is the Qt source release above plus the patches in this repository, and ``build-qt.ps1`` rebuilds these libraries from them. libwebrtc is BSD-licensed; its third-party notices are ``libwebrtc/LICENSE.md`` in the archive.

**Files.** Every library and object is packed whole (``-r2``: the release without the suffix packed 176 Qt libraries and objects as empty stubs, which fail the link with LNK1136 once a build uses one). ``desktop-native/tools/toolchain.lock.json`` lists every asset's size and SHA-256; ``*.contents.sha256`` list every file inside. ``desktop-native/tools/fetch-toolchain.ps1`` downloads, checks and unpacks them; ``desktop-native/tools/package-toolchain.ps1`` recreates them.
"@
    $bodyFile = Join-Path $OutDir "release-body.md"
    Set-Content -Encoding utf8NoBOM $bodyFile $body
    gh release view $Tag --repo $Repo *> $null
    if ($LASTEXITCODE) {
        gh release create $Tag --repo $Repo --prerelease --latest=false --title "Native toolchain: Qt $qtVersion (yap) + libwebrtc $WebrtcBuild" --notes-file $bodyFile
        if ($LASTEXITCODE) { throw "gh release create failed" }
    } else {
        gh release edit $Tag --repo $Repo --prerelease --latest=false --notes-file $bodyFile
        if ($LASTEXITCODE) { throw "gh release edit failed" }
    }
    foreach ($a in $assets) {
        gh release upload $Tag --repo $Repo --clobber (Join-Path $OutDir $a.name)
        if ($LASTEXITCODE) { throw "upload of $($a.name) failed" }
    }
    gh release upload $Tag --repo $Repo --clobber $Lock
    if ($LASTEXITCODE) { throw "upload of the lock failed" }
    Write-Host "==> uploaded to https://github.com/$Repo/releases/tag/$Tag"
}
