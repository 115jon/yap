<#
.SYNOPSIS
    Carries the Tauri bridge's updater manifest (latest.json) on native
    releases, and checks what releases/latest serves.

.DESCRIPTION
    Tauri installs (3.13 and older) update from
    https://github.com/<repo>/releases/latest/download/latest.json. Once a
    native release is GitHub's "latest", that file must be the bridge
    release's latest.json (packaging/tauri-bridge.json, "tag"), byte for
    byte: it sends them to the bridge's YapSetup.exe by its tag URL, and the
    bridge then installs the native app.

    -Mode Fetch
        Downloads the bridge release's latest.json to -OutFile and checks it:
        version is the tag's version, the windows-x86_64 url is
        .../releases/download/<tag>/YapSetup.exe, the minisign signature's
        key id is the pinned pubkey's, pub_date is UTC, and a HEAD on the url
        returns 200. Throws on any failure.

    -Mode VerifyLatest
        Checks what releases/latest serves now. -ExpectedLatest, when given,
        must be the latest tag.
          - A native release (v<major >= 4>): latest/download/latest.json
            equals the bridge's (fetched and checked as above), and
            latest/download/ serves Yap.App-win-Setup.exe, YapSetup.exe and
            releases.win.json.
          - A Tauri release (v3.x and older, before the cutover): its
            latest.json passes the same checks against its own tag.
          - Anything else (a desktop-native-v* release, say) serves no
            latest.json, which strands Tauri installs: a failure.
        Throws with every failure listed.

    The signature is not cryptographically verified (no minisign verifier
    here): the key id must match, and the bytes are the ones the Tauri
    release workflow published and checked (validate-latest-json.ps1, on the
    archive/tauri-desktop branch).

.EXAMPLE
    ./native/tools/tauri-bridge.ps1 -Mode Fetch -OutFile $env:RUNNER_TEMP/bridge/latest.json
    ./native/tools/tauri-bridge.ps1 -Mode VerifyLatest -ExpectedLatest v4.0.0
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateSet("Fetch", "VerifyLatest")][string]$Mode,
    [string]$Repo = $(if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { "115jon/yap" }),
    # Overrides the config's tag (the repo variable TAURI_BRIDGE_TAG).
    [string]$BridgeTag = "",
    [string]$OutFile = "",
    [string]$ExpectedLatest = "",
    [string]$Config = (Join-Path $PSScriptRoot "../packaging/tauri-bridge.json"),
    # Attempts for the public URLs, which can lag a publish by a few seconds.
    [int]$Attempts = 6
)

$ErrorActionPreference = "Stop"
$NativeSetup = "Yap.App-win-Setup.exe"
$NativeFeed = "releases.win.json"

# A minisign key or signature line is base64 of: 2-byte algorithm, 8-byte key id, then the rest.
function Get-MinisignKeyId([string]$Base64Line) {
    $bytes = [Convert]::FromBase64String($Base64Line.Trim())
    if ($bytes.Length -lt 10) { throw "minisign data is too short" }
    return ($bytes[2..9] | ForEach-Object { $_.ToString("x2") }) -join ""
}

# The updater's pubkey and signature are base64 minisign files: comment line, base64 line, ...
function Get-MinisignDataLine([string]$Base64File) {
    $text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Base64File.Trim()))
    $lines = $text -split "\r?\n" | Where-Object { $_ -and -not $_.StartsWith("untrusted comment:") -and -not $_.StartsWith("trusted comment:") }
    if (-not $lines) { throw "no minisign data line found" }
    return @($lines)[0]
}

function Invoke-Retry([scriptblock]$Action, [string]$What) {
    for ($i = 1; ; $i++) {
        try { return & $Action }
        catch {
            if ($i -ge $Attempts) { throw "$What failed after $i attempts: $($_.Exception.Message)" }
            Start-Sleep -Seconds ([math]::Min(5 * $i, 20))
        }
    }
}

# The status after redirects (GitHub sends release downloads to its asset host).
function Get-HeadStatus([string]$Url) {
    try {
        return Invoke-Retry -What "HEAD $Url" -Action {
            $r = Invoke-WebRequest -Method Head -Uri $Url -MaximumRedirection 10 -SkipHttpErrorCheck -UseBasicParsing
            if ($r.StatusCode -ge 500 -or $r.StatusCode -eq 404) { throw "HTTP $($r.StatusCode)" }
            $r.StatusCode
        }
    } catch { return $_.Exception.Message }
}

function Get-UrlBytes([string]$Url) {
    $tmp = [IO.Path]::GetTempFileName()
    try {
        Invoke-Retry -What "GET $Url" -Action {
            Invoke-WebRequest -Uri $Url -OutFile $tmp -MaximumRedirection 10 -UseBasicParsing | Out-Null
        } | Out-Null
        return [IO.File]::ReadAllBytes($tmp)
    } finally { Remove-Item -LiteralPath $tmp -ErrorAction SilentlyContinue }
}

function Get-TagVersion([string]$Tag) {
    $m = [regex]::Match($Tag, '^v(\d+)\.(\d+)\.(\d+)$')
    if (-not $m.Success) { return $null }
    return [pscustomobject]@{ Text = $Tag.Substring(1); Major = [int]$m.Groups[1].Value }
}

# Problems with a Tauri latest.json (bytes) published on $Tag; an empty list when it is good.
function Test-TauriManifest([byte[]]$Bytes, [string]$Tag, [string]$PubKey) {
    $errors = [System.Collections.Generic.List[string]]::new()
    $text = [Text.Encoding]::UTF8.GetString($Bytes)
    try { $json = $text | ConvertFrom-Json } catch { $errors.Add("latest.json is not JSON: $($_.Exception.Message)"); return , $errors }
    $version = (Get-TagVersion $Tag).Text
    if ($json.version -ne $version) { $errors.Add("version '$($json.version)' != '$version' (tag $Tag)") }
    # ConvertFrom-Json turns ISO timestamps into DateTime, so read pub_date from the text.
    $pubDate = if ($text -match '"pub_date"\s*:\s*"([^"]*)"') { $Matches[1] } else { "" }
    if ($pubDate -notmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$') { $errors.Add("pub_date '$pubDate' is not yyyy-MM-ddTHH:mm:ssZ") }
    $platform = $json.platforms."windows-x86_64"
    if (-not $platform) { $errors.Add("platforms.windows-x86_64 is missing"); return , $errors }
    $expectedUrl = "https://github.com/$Repo/releases/download/$Tag/$($script:Bridge.installer)"
    if ($platform.url -ne $expectedUrl) { $errors.Add("url '$($platform.url)' != '$expectedUrl'") }
    try {
        $sigKey = Get-MinisignKeyId (Get-MinisignDataLine $platform.signature)
        $pubKeyId = Get-MinisignKeyId (Get-MinisignDataLine $PubKey)
        if ($sigKey -ne $pubKeyId) { $errors.Add("signature key id $sigKey != updater pubkey key id $pubKeyId") }
    } catch { $errors.Add("signature or pubkey is not minisign base64: $($_.Exception.Message)") }
    if ($platform.url) {
        $status = Get-HeadStatus $platform.url
        if ($status -ne 200) { $errors.Add("HEAD $($platform.url) -> $status") }
    }
    return , $errors
}

# Downloads and checks the bridge's latest.json; returns its bytes.
function Get-BridgeManifest {
    $tag = $script:Bridge.tag
    if ($BridgeTag -and $BridgeTag -ne $tag) {
        Write-Host "::warning::TAURI_BRIDGE_TAG is $BridgeTag; packaging/tauri-bridge.json says $tag. Using $BridgeTag."
        $tag = $BridgeTag
    }
    $v = Get-TagVersion $tag
    if (-not $v) { throw "Bridge tag '$tag' is not v<x.y.z>" }
    if ($v.Major -ge 4) { throw "Bridge tag $tag is a native version; the bridge is a Tauri release (v3.x)" }

    $release = gh api "repos/$Repo/releases/tags/$tag" 2>$null | ConvertFrom-Json
    if ($LASTEXITCODE -or -not $release) { throw "No published release $tag in $Repo (a draft does not count)" }
    if ($release.draft -or $release.prerelease) { throw "Bridge release $tag is a draft or a pre-release" }
    foreach ($name in "latest.json", $script:Bridge.installer) {
        if ($release.assets.name -notcontains $name) { throw "Bridge release $tag has no $name" }
    }

    $dir = Join-Path ([IO.Path]::GetTempPath()) "tauri-bridge-$([guid]::NewGuid().ToString('n'))"
    New-Item -ItemType Directory -Force $dir | Out-Null
    try {
        Invoke-Retry -What "gh release download $tag latest.json" -Action {
            gh release download $tag --repo $Repo --pattern latest.json --dir $dir --clobber
            if ($LASTEXITCODE) { throw "gh exited $LASTEXITCODE" }
        } | Out-Null
        $bytes = [IO.File]::ReadAllBytes((Join-Path $dir "latest.json"))
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue }

    $errors = Test-TauriManifest -Bytes $bytes -Tag $tag -PubKey $script:Bridge.pubkey
    if ($errors.Count) { throw "Bridge $tag latest.json failed its checks:`n  - $($errors -join "`n  - ")" }
    $sha = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
    Write-Host "==> bridge $tag latest.json OK ($($bytes.Length) bytes, sha256 $sha)"
    return , $bytes
}

# Never name another variable $bridge: names are case-insensitive, so it would overwrite this.
$script:Bridge = Get-Content -Raw -LiteralPath $Config | ConvertFrom-Json
foreach ($key in "tag", "installer", "pubkey") {
    if (-not $script:Bridge.$key) { throw "$Config has no '$key'" }
}

if ($Mode -eq "Fetch") {
    if (-not $OutFile) { throw "-Mode Fetch needs -OutFile" }
    $bytes = Get-BridgeManifest
    $parent = Split-Path -Parent $OutFile
    if ($parent) { New-Item -ItemType Directory -Force $parent | Out-Null }
    [IO.File]::WriteAllBytes($OutFile, $bytes)
    Write-Host "==> wrote $OutFile"
    exit 0
}

# VerifyLatest
$problems = [System.Collections.Generic.List[string]]::new()
$latest = Invoke-Retry -What "read the latest release" -Action {
    $t = gh api "repos/$Repo/releases/latest" --jq .tag_name 2>$null
    if ($LASTEXITCODE -or -not $t) { throw "gh exited $LASTEXITCODE" }
    $t
}
Write-Host "==> latest release: $latest"
if ($ExpectedLatest -and $latest -ne $ExpectedLatest) {
    $problems.Add("latest is $latest, expected $ExpectedLatest")
}
$base = "https://github.com/$Repo/releases/latest/download"
$v = Get-TagVersion $latest
if ($v -and $v.Major -ge 4) {
    try {
        $bridgeBytes = Get-BridgeManifest
        $served = Get-UrlBytes "$base/latest.json"
        if ([Convert]::ToBase64String($served) -ne [Convert]::ToBase64String($bridgeBytes)) {
            $problems.Add("$base/latest.json ($($served.Length) bytes) is not the bridge's latest.json ($($bridgeBytes.Length) bytes)")
        }
    } catch { $problems.Add($_.Exception.Message) }
    foreach ($name in $NativeSetup, $script:Bridge.installer, $NativeFeed) {
        $status = Get-HeadStatus "$base/$name"
        if ($status -ne 200) { $problems.Add("HEAD $base/$name -> $status") }
    }
} elseif ($v) {
    # Before the cutover: a Tauri release is latest and serves its own latest.json.
    try {
        $served = Get-UrlBytes "$base/latest.json"
        $errors = Test-TauriManifest -Bytes $served -Tag $latest -PubKey $script:Bridge.pubkey
        foreach ($e in $errors) { $problems.Add("Tauri latest $latest latest.json: $e") }
    } catch { $problems.Add($_.Exception.Message) }
} else {
    $problems.Add("latest is $latest, which is neither a Tauri v3.x nor a native v4+ release: $base/latest.json is missing and Tauri installs cannot update")
}

if ($problems.Count) {
    throw "releases/latest checks failed:`n  - $($problems -join "`n  - ")"
}
Write-Host "==> releases/latest ($latest) OK"
exit 0
