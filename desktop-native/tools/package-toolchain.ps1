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
# What is left out of Qt is in tools/toolchain-qt.exclude: docs and SBOMs
# dropped, and the libraries a clean build.ps1 never reads packed as empty
# stubs (that file says how they were measured).
# Each archive's file list and per-file SHA-256 go to <name>.contents.sha256:
# packing the same toolchain again gives the same contents files, and, with
# the same 7-Zip version, the same archive bytes (no access or creation
# times are stored; files are added in sorted order).
param(
    [string]$QtDir = "F:/ct/qt-custom-6.11p8/6.11.3-yap",
    [string]$WebrtcDir = "F:/ct/libwebrtc/win-x64-release-webrtc-a545de4/win-x64-release",
    [string]$OutDir = "F:/ct/yapn/toolchain",
    [string]$Tag = "native-toolchain-qt6.11.3-yap-webrtc-a545de4",
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
$stubs = @{}
foreach ($line in Get-Content $Exclude) {
    $line = $line.Trim()
    if (-not $line -or $line.StartsWith("#")) { continue }
    $kind, $glob = $line -split '\s+', 2
    switch ($kind) {
        "drop" { $drops += ConvertTo-Regex $glob }
        "stub" { $stubs[$glob] = $true }
        default { throw "toolchain-qt.exclude: unknown rule '$line'" }
    }
}

# Every file under $root as Rel (with "/"), Path and Stub, sorted; dropped
# ones left out.
function Get-Files([string]$root, [string[]]$skip, [hashtable]$stub) {
    $full = (Resolve-Path $root).Path
    $files = @(Get-ChildItem -Recurse -File -Force $full | ForEach-Object {
        $rel = $_.FullName.Substring($full.Length + 1) -replace '\\', '/'
        $out = $false
        foreach ($re in $skip) { if ($rel -match $re) { $out = $true; break } }
        if (-not $out) { [pscustomobject]@{ Rel = $rel; Path = $_.FullName; Stub = $stub.ContainsKey($rel) } }
    } | Sort-Object Rel)
    $known = @{}
    foreach ($f in $files) { $known[$f.Rel] = $true }
    $missing = @($stub.Keys | Where-Object { -not $known.ContainsKey($_) })
    if ($missing) { throw "Stubs not in ${root}: $($missing -join ', ')" }
    return $files
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
            if ($f.Stub) {
                New-Item -ItemType File $dest | Out-Null
                (Get-Item $dest).LastWriteTimeUtc = [datetime]::new(2000, 1, 1, 0, 0, 0, [DateTimeKind]::Utc)
                continue
            }
            try { New-Item -ItemType HardLink -Path $dest -Target $f.Path | Out-Null }
            catch { Copy-Item -LiteralPath $f.Path $dest }
        }
        return $dir
    }

    $components = [ordered]@{}
    $qtFiles = @(Get-Files $QtDir $drops $stubs)
    $components["qt"] = New-Stage "qt" $qtFiles
    # A static Qt is not relocatable: qmake reports the build PC's prefix
    # unless a qt.conf beside it says otherwise (cxx-qt asks qmake for paths).
    Set-Content -Encoding ascii (Join-Path $components["qt"] "bin\qt.conf") "[Paths]`r`nPrefix = ..`r`n"
    $webrtcFiles = @(Get-Files $WebrtcDir @() @{})
    $components["libwebrtc"] = New-Stage "libwebrtc" $webrtcFiles

    Get-ChildItem $OutDir -File | Where-Object { $_.Name -match '\.7z(\.\d+)?$|\.contents\.sha256$' } | Remove-Item
    $assets = @()
    foreach ($name in $components.Keys) {
        $dir = $components[$name]
        $archive = Join-Path $OutDir $names[$name]

        # The contents list: every file's SHA-256 and path, sorted.
        $rows = Get-ChildItem -Recurse -File $dir | ForEach-Object {
            $rel = "$name/" + ($_.FullName.Substring($dir.Length + 1) -replace '\\', '/')
            [pscustomobject]@{ Rel = $rel; Hash = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant(); Length = $_.Length }
        } | Sort-Object Rel
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
            files   = $qtFiles.Count
            stubs   = @($qtFiles | Where-Object Stub).Count
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

- **Qt $qtVersion, static, patched** (``$($names.qt).*``): built with ``desktop-native/patches/qt/build-qt.ps1`` from the official ``qt-everywhere-src-$qtVersion`` source (https://download.qt.io/official_releases/qt/), modules qtbase, qtshadertools, qtdeclarative, qtsvg, qtimageformats and qtmultimedia, ``-static -static-runtime -release -optimize-size -ltcg``, MSVC $msvc, with the patches in ``desktop-native/patches/qt/$qtVersion/``. Docs and SBOMs dropped, and the libraries ``desktop-native/build.ps1`` never reads kept as empty stubs (``desktop-native/tools/toolchain-qt.exclude``); ``bin/qt.conf`` added so the prefix is relocatable.
- **libwebrtc $WebrtcBuild** (``$($names.libwebrtc).*``): LiveKit's prebuilt ``webrtc-win-x64-release`` (the one ``webrtc-sys`` 0.3.52 downloads), unmodified. Yap's changes to the Rust bindings are source patches in ``desktop-native/patches/webrtc-sys`` and ``desktop-native/patches/libwebrtc``, applied by ``desktop-native/patches/vendor.ps1``.

**Licences.** Qt is used under the GNU LGPL v3 (and GPL v3 for its tools); the corresponding source is the Qt source release above plus the patches in this repository, and ``build-qt.ps1`` rebuilds these libraries from them. libwebrtc is BSD-licensed; its third-party notices are ``libwebrtc/LICENSE.md`` in the archive.

**Files.** ``desktop-native/tools/toolchain.lock.json`` lists every asset's size and SHA-256; ``*.contents.sha256`` list every file inside. ``desktop-native/tools/fetch-toolchain.ps1`` downloads, checks and unpacks them; ``desktop-native/tools/package-toolchain.ps1`` recreates them.
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
