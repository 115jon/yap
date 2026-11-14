<#
.SYNOPSIS
  One case of the Tauri -> native cutover E2E (.github/workflows/desktop-cutover-e2e.yml).

.DESCRIPTION
  Runs on a throwaway hosted Windows runner only: it installs the real
  Tauri app, updates it, runs the 3.14 bridge (desktop/src-tauri/src/
  native_handoff on the archive/tauri-desktop branch) headless and lets the
  native app replace it. Never run it
  on a machine you care about.

  Every case starts the same way:
    1. install v3.13.0's YapSetup.exe /S (which starts the 3.13 app);
    2. update to the bridge release: through the 3.13 app's own updater when
       the bridge release is GitHub's latest (case "happy"), otherwise, or if
       that does not happen in time, by running the bridge's YapSetup.exe
       with the Tauri updater's arguments (/P /R /UPDATE /ARGS /S);
    3. stop the Tauri app and seed what the native import reads
       (%APPDATA%\Yap\desktop-settings.json, .window-state.json and the CEF
       profile's localStorage);
    4. copy the installed app-<version> folder to a folder not named app-*
       (the bridge reads YAP_HANDOFF_RELEASE only outside an install) and run
       it with --yap-native-handoff.

  Cases: happy, tampered, unsigned, switch-off, native-installed, rerun.
  Results go to <Out>\checks.json and the job summary; the script exits 1 if
  any check failed.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [ValidateSet('happy', 'tampered', 'unsigned', 'switch-off', 'native-installed', 'rerun')]
  [string] $Case,
  [Parameter(Mandatory)] [string] $BridgeTag,
  [Parameter(Mandatory)] [string] $NativeTag,
  [string] $OldTag = 'v3.13.0',
  [Parameter(Mandatory)] [string] $FixtureExe,
  [string] $Repo = '115jon/yap',
  [bool] $BridgeIsLatest = $false,
  [string] $Out = (Join-Path $env:RUNNER_TEMP 'cutover-out')
)

$ErrorActionPreference = 'Stop'
if (-not $env:GITHUB_ACTIONS -or -not $env:RUNNER_TEMP) {
  throw 'run-case.ps1 installs and uninstalls Yap for this user and rewrites HKCU: it runs on a throwaway GitHub-hosted runner only.'
}
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version 1

# ── Paths ────────────────────────────────────────────────────────────────

$Work = Join-Path $env:RUNNER_TEMP 'cutover'
$Downloads = Join-Path $Work 'downloads'
$Feed = Join-Path $Work 'feed'
New-Item -ItemType Directory -Force $Out, $Work, $Downloads, $Feed | Out-Null

$TauriRoot = Join-Path $env:LOCALAPPDATA 'Yap'
$TauriData = Join-Path $env:APPDATA 'Yap'
$NativeRoot = Join-Path $env:LOCALAPPDATA 'Yap.App'
$NativeExe = Join-Path $NativeRoot 'current\Yap.exe'
$NativeData = Join-Path $env:LOCALAPPDATA 'Yap.Data'
$HandoffWork = Join-Path $env:TEMP 'yap-native-handoff'
$Marker = Join-Path $TauriData 'native-handoff.json'
$InstallerLog = Join-Path $NativeData 'logs\installer.log'

$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$SchemeCommandKey = 'HKCU:\Software\Classes\yap\shell\open\command'

$BridgeVersion = $BridgeTag.TrimStart('v')
# desktop-native-vX.Y.Z (before 4.0.0) or release-please's vX.Y.Z (4.0.0 on).
$NativeVersion = ($NativeTag -replace '^(desktop-native-)?v', '')
# The override's tag must be vX.Y.Z with X >= 4 (policy::accept): a v4+
# release is itself, a native 0.A.B release stands in as v4.A.B.
$nv = $NativeVersion.Split('.')
$NativeMajor = [int]$nv[0]
$OverrideTag = if ($NativeMajor -ge 4) { "v$NativeVersion" } else { "v4.$($nv[1]).$($nv[2])" }
$OverrideVersion = $OverrideTag.TrimStart('v')

$SignerName = 'Jonathan Martinez'
$IssuerPrefix = 'Microsoft ID Verified CS'

# ── Checks and evidence ──────────────────────────────────────────────────

$script:Checks = [System.Collections.Generic.List[object]]::new()
$script:FeedServer = $null
$script:RunKeyBefore = $null
$script:RunKeyAtBridge = $null
$script:UpdatePath = 'n/a'
$script:Notes = [System.Collections.Generic.List[string]]::new()
$script:T0 = Get-Date

function Log([string] $text) {
  $stamp = '{0,7:n1}s' -f ((Get-Date) - $script:T0).TotalSeconds
  Write-Host "[$stamp] $text"
}

function Note([string] $text) {
  Log "NOTE: $text"
  $script:Notes.Add($text)
}

# A check: Pass, Fail, or Warn (reported, not failing the job).
function Check([string] $name, [bool] $ok, [string] $detail = '', [switch] $Soft) {
  $status = if ($ok) { 'pass' } elseif ($Soft) { 'warn' } else { 'fail' }
  $script:Checks.Add([pscustomobject]@{ name = $name; status = $status; detail = $detail })
  $mark = @{ pass = 'PASS'; warn = 'WARN'; fail = 'FAIL' }[$status]
  if ($status -eq 'fail') {
    Write-Host "::error title=${Case}/${name}::$detail"
  }
  elseif ($status -eq 'warn') {
    Write-Host "::warning title=${Case}/${name}::$detail"
  }
  Log "$mark  $name  $detail"
}

function Save-Screenshot([string] $name) {
  try {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bmp.Size)
    $path = Join-Path $Out "screenshots\$name.png"
    New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Log "screenshot $name ($($bounds.Width)x$($bounds.Height))"
  }
  catch {
    Log "screenshot $name failed: $($_.Exception.Message)"
  }
}

function Copy-Evidence([string] $source, [string] $dest) {
  if (Test-Path -LiteralPath $source) {
    $target = Join-Path $Out $dest
    New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
    Copy-Item -LiteralPath $source -Destination $target -Recurse -Force -ErrorAction SilentlyContinue
  }
}

function Export-Registry([string] $label) {
  $file = Join-Path $Out "registry\$label.txt"
  New-Item -ItemType Directory -Force (Split-Path $file) | Out-Null
  $lines = foreach ($key in @("$UninstallKey\Yap", "$UninstallKey\Yap.App", $SchemeCommandKey, 'HKCU:\Software\Classes\yap-native\shell\open\command', 'HKCU:\Software\115jon\Yap')) {
    "[$key]"
    if (Test-Path $key) {
      (Get-ItemProperty $key | Select-Object * -ExcludeProperty PS* | Format-List | Out-String).Trim()
    }
    else { '(absent)' }
    ''
  }
  $lines += "[$RunKey]"
  $lines += (Get-ItemProperty $RunKey -ErrorAction SilentlyContinue | Select-Object * -ExcludeProperty PS* | Format-List | Out-String).Trim()
  $lines | Set-Content $file
}

function Wait-Until([scriptblock] $condition, [int] $seconds, [int] $pollMs = 1000) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    if (& $condition) { return $true }
    Start-Sleep -Milliseconds $pollMs
  }
  return [bool](& $condition)
}

function Read-Json([string] $path) {
  if (-not (Test-Path -LiteralPath $path)) { return $null }
  try { Get-Content -LiteralPath $path -Raw | ConvertFrom-Json } catch { $null }
}

function Get-RegValue([string] $key, [string] $name) {
  try { (Get-ItemProperty -LiteralPath $key -ErrorAction Stop).$name } catch { $null }
}

function Get-Signer([string] $path) {
  $sig = Get-AuthenticodeSignature -LiteralPath $path
  $cert = $sig.SignerCertificate
  [pscustomobject]@{
    status      = "$($sig.Status)"
    subject     = if ($cert) { $cert.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false) } else { $null }
    issuer      = if ($cert) { $cert.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $true) } else { $null }
    thumbprint  = if ($cert) { $cert.Thumbprint } else { $null }
    timestamped = [bool]$sig.TimeStamperCertificate
  }
}

function Test-SignedByYap([string] $path) {
  $s = Get-Signer $path
  $ok = $s.status -eq 'Valid' -and $s.subject -eq $SignerName -and "$($s.issuer)".StartsWith($IssuerPrefix) -and $s.timestamped
  return @($ok, "status=$($s.status) subject=$($s.subject) issuer=$($s.issuer) ts=$($s.timestamped) thumb=$($s.thumbprint)")
}

function Get-ProcessesUnder([string] $folder) {
  $prefix = $folder.TrimEnd('\') + '\'
  @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) })
}

function Get-NativeProcesses {
  @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath -ieq $NativeExe })
}

function Stop-Tauri {
  $procs = Get-ProcessesUnder $TauriRoot
  foreach ($p in $procs) {
    Log "stopping Tauri process $($p.ProcessId) $($p.ExecutablePath)"
    Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
  }
  [void](Wait-Until { (Get-ProcessesUnder $TauriRoot).Count -eq 0 } 30)
}

function Get-HandoffLog {
  $p = Join-Path $HandoffWork 'handoff.log'
  if (Test-Path $p) { Get-Content $p -Raw } else { '' }
}

function Get-InstallerLog {
  if (Test-Path $InstallerLog) { Get-Content $InstallerLog -Raw } else { '' }
}

function Get-FeedRequests {
  $log = Join-Path $Work 'feed-server.err.log'
  if (-not (Test-Path $log)) { return 0 }
  @(Select-String -Path $log -Pattern 'GET /Yap\.App-win-Setup\.exe' -ErrorAction SilentlyContinue).Count
}

# ── Release assets ───────────────────────────────────────────────────────

function Get-AssetInfo([string] $tag, [string] $name) {
  $json = gh api "repos/$Repo/releases/tags/$tag" | ConvertFrom-Json
  $asset = $json.assets | Where-Object name -EQ $name
  if (-not $asset) { throw "$tag has no asset $name" }
  if (-not "$($asset.digest)".StartsWith('sha256:')) { throw "$tag/$name has no sha256 digest" }
  [pscustomobject]@{
    tag    = $tag
    name   = $name
    size   = [long]$asset.size
    sha256 = $asset.digest.Substring(7).ToLowerInvariant()
    url    = $asset.browser_download_url
    draft  = [bool]$json.draft
  }
}

function Get-Asset([string] $tag, [string] $name) {
  $info = Get-AssetInfo $tag $name
  $dir = Join-Path $Downloads $tag
  New-Item -ItemType Directory -Force $dir | Out-Null
  $path = Join-Path $dir $name
  if (-not (Test-Path $path)) {
    gh release download $tag --repo $Repo --pattern $name --dir $dir --clobber
    if ($LASTEXITCODE -ne 0) { throw "download of $tag/$name failed" }
  }
  $hash = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant()
  if ($hash -ne $info.sha256) { throw "$tag/${name}: SHA-256 $hash, GitHub says $($info.sha256)" }
  Log "downloaded $tag/$name ($($info.size) bytes, sha256 $hash)"
  $info | Add-Member -NotePropertyName path -NotePropertyValue $path -PassThru
}

# ── Steps ────────────────────────────────────────────────────────────────

function Invoke-Installer([string] $exe, [string[]] $arguments, [int] $timeoutSec = 600) {
  Log "running $exe $($arguments -join ' ')"
  $p = Start-Process -FilePath $exe -ArgumentList $arguments -PassThru
  $null = $p.Handle
  if (-not $p.WaitForExit($timeoutSec * 1000)) { throw "$exe did not finish in $timeoutSec s" }
  Log "$([IO.Path]::GetFileName($exe)) exited with $($p.ExitCode)"
  return $p.ExitCode
}

function Get-TauriVersion {
  Get-RegValue "$UninstallKey\Yap" 'DisplayVersion'
}

function Install-OldTauri([bool] $launch) {
  $old = Get-Asset $OldTag 'YapSetup.exe'
  # /S starts the app unless /NoLaunch. A started 3.13 updates itself to
  # the latest release within seconds, so it starts only where that update
  # is the one under test: a second Setup run beside it deletes the first
  # one's staging folder (ActivationManager.PrepareStagingDirectory) and
  # both fail.
  $installArgs = if ($launch) { @('/S') } else { @('/S', '/NoLaunch') }
  $code = Invoke-Installer $old.path $installArgs
  Check 'install-3.13' ($code -eq 0 -and (Get-TauriVersion) -eq $OldTag.TrimStart('v')) "exit $code, Uninstall\Yap DisplayVersion=$(Get-TauriVersion)"
  if ($code -ne 0) { throw "the $OldTag install failed" }
  if ($launch) {
    $up = Wait-Until { (Get-ProcessesUnder $TauriRoot).Count -gt 0 } 60
    Log "3.13 app running after the silent install: $up"
    Start-Sleep -Seconds 10
    Save-Screenshot '01-tauri-3.13'
  }
}

function Update-ToBridge([bool] $tryRealUpdater) {
  $setup = Get-Asset $BridgeTag 'YapSetup.exe'
  $done = { (Get-TauriVersion) -eq $BridgeVersion -and (Test-Path (Join-Path $TauriRoot "app-$BridgeVersion")) -and -not (Get-Process -Name 'YapSetup*' -ErrorAction SilentlyContinue) }
  $path = $null
  if ($tryRealUpdater) {
    Log "waiting for the $OldTag app's own updater to install $BridgeTag"
    if (Wait-Until $done 600 2000) {
      $path = 'real-updater'
    }
    else {
      Check 'real-updater-used' $false "the $OldTag app's updater did not install $BridgeTag within 10 min; falling back to the Setup with the updater's arguments" -Soft
      Save-Screenshot '02-real-updater-timeout'
    }
  }
  if (-not $path) {
    # What tauri-plugin-updater runs for installMode "passive" plus
    # installerArgs ["/S"] (tauri.conf.json): <setup> /P /R /UPDATE /ARGS /S,
    # after the app has exited (std::process::exit right after ShellExecute).
    Stop-Tauri
    $code = Invoke-Installer $setup.path @('/P', '/R', '/UPDATE', '/ARGS', '/S')
    if ($code -ne 0) { throw "the $BridgeTag update exited with $code" }
    [void](Wait-Until $done 60)
    $path = 'direct-setup'
  }
  Check 'update-to-bridge' ((& $done)) "path=$path, DisplayVersion=$(Get-TauriVersion)"
  $script:UpdatePath = $path
  # The updated app starts itself; let it come up, then stop it so its CEF
  # profile is closed while the seeds are written.
  [void](Wait-Until { (Get-ProcessesUnder (Join-Path $TauriRoot "app-$BridgeVersion")).Count -gt 0 } 60)
  Start-Sleep -Seconds 15
  Save-Screenshot '02-tauri-bridge-version'
  Stop-Tauri
}

# Non-default values, except startMinimized (its default, false): on, the
# window would start in the tray and the screenshots would show nothing.
$SeededSettings = [ordered]@{
  openOnStartup        = $true
  startMinimized       = $false
  closeToTray          = $false
  desktopNotifications = $false
  gameCaptureHook      = $false
}
# At least the native window's minimum (940x560, Main.qml), so Qt does not
# clamp it and re-save window-state.json; fits the runners' 1024x768.
$SeededWindow = @{ x = 20; y = 20; width = 960; height = 600 }
$SeededTheme = 'miku-dark'
$SeededSounds = [ordered]@{
  soundsEnabled     = $false
  soundVolume       = 42
  soundboardVolume  = 17
  muteDeafen        = $false
  calls             = $false
  customSounds      = $true
}

function Write-Seeds([bool] $tauriRunValue) {
  $script:RunKeyBefore = Test-Path $RunKey
  Note "HKCU Run key present before seeding: $($script:RunKeyBefore)"
  if ($tauriRunValue) {
    # What the Tauri app writes with openOnStartup on (repair at launch,
    # build_windows_startup_command: the root stub and --autostart).
    if (-not (Test-Path $RunKey)) { New-Item -Path $RunKey -Force | Out-Null }
    Set-ItemProperty -Path $RunKey -Name 'Yap' -Value "`"$TauriRoot\Yap.exe`" --autostart"
  }
  elseif (Test-Path $RunKey) {
    # No Run key at all, as on a fresh profile (some windows-2025 images):
    # the native app has to create it to turn autostart on.
    Remove-Item -Path $RunKey -Recurse -Force
    Note 'removed the HKCU Run key: this case covers a profile without one'
  }
  New-Item -ItemType Directory -Force $TauriData | Out-Null
  Copy-Evidence (Join-Path $TauriData 'desktop-settings.json') 'seed\tauri-wrote\desktop-settings.json'
  Copy-Evidence (Join-Path $TauriData '.window-state.json') 'seed\tauri-wrote\.window-state.json'

  # desktop-settings.json: keep what the app wrote, override the five fields
  # the import reads with non-default values.
  $settingsPath = Join-Path $TauriData 'desktop-settings.json'
  $settings = [ordered]@{}
  $existing = Read-Json $settingsPath
  if ($existing) { foreach ($p in $existing.PSObject.Properties) { $settings[$p.Name] = $p.Value } }
  foreach ($k in $SeededSettings.Keys) { $settings[$k] = $SeededSettings[$k] }
  $settings | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8NoBOM $settingsPath

  # .window-state.json (tauri-plugin-window-state, physical pixels).
  $window = [ordered]@{
    main = [ordered]@{
      width = $SeededWindow.width; height = $SeededWindow.height; x = $SeededWindow.x; y = $SeededWindow.y
      prev_x = $SeededWindow.x; prev_y = $SeededWindow.y; maximized = $false; visible = $true; decorated = $false; fullscreen = $false
    }
  }
  $window | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8NoBOM (Join-Path $TauriData '.window-state.json')

  # The CEF profile's localStorage (the first place the import looks),
  # replaced by a database holding the web UI's keys for its origin.
  $leveldb = Join-Path $TauriData 'cef\Default\Local Storage\leveldb'
  if (Test-Path $leveldb) {
    Copy-Evidence $leveldb 'seed\tauri-wrote\leveldb'
    # The database the Tauri app's CEF wrote, opened the way the import
    # opens it (it holds no app keys: nobody signed in).
    $listing = & $FixtureExe --read $leveldb 2>&1 | Out-String
    Check 'cef-localstorage-readable' ($LASTEXITCODE -eq 0) ($listing.Trim() -replace '\s+', ' ') -Soft
    Remove-Item -Recurse -Force $leveldb
  }
  $store = [ordered]@{
    state   = [ordered]@{ currentUser = 'e2e-user'; userSettings = [ordered]@{ 'e2e-user' = $SeededSounds } }
    version = 1
  }
  $items = [ordered]@{
    'theme'                        = $SeededTheme
    'sound-settings-storage'       = ($store | ConvertTo-Json -Depth 6 -Compress)
    'rm-voice-switch-skip-confirm' = 'true'
    'rm-start-call-skip-confirm'   = 'true'
  }
  $itemsFile = Join-Path $Work 'localstorage-items.json'
  $items | ConvertTo-Json -Depth 3 | Set-Content -Encoding utf8NoBOM $itemsFile
  & $FixtureExe $leveldb 'http://tauri.localhost' $itemsFile
  if ($LASTEXITCODE -ne 0) { throw 'the localStorage fixture failed' }

  Copy-Evidence $settingsPath 'seed\desktop-settings.json'
  Copy-Evidence (Join-Path $TauriData '.window-state.json') 'seed\.window-state.json'
  Copy-Evidence $itemsFile 'seed\localstorage-items.json'
  Log 'seeded the Tauri settings, window state and localStorage'
}

function New-BridgeCopy {
  $installed = Join-Path $TauriRoot "app-$BridgeVersion"
  # Not app-*: versioned_windows_install_root_from_exe_path, so the copy is
  # not an installed one and reads YAP_HANDOFF_RELEASE.
  $copy = Join-Path $Work "bridge-$BridgeVersion"
  if (-not (Test-Path $copy)) {
    Copy-Item -Recurse $installed $copy
  }
  $exe = Join-Path $copy 'Yap.exe'
  if (-not (Test-Path $exe)) {
    $exe = (Get-ChildItem $copy -Filter *.exe | Where-Object { $_.VersionInfo.ProductVersion -like "$BridgeVersion*" } | Select-Object -First 1).FullName
  }
  $ver = (Get-Item $exe).VersionInfo
  $signed = Test-SignedByYap $exe
  Log "bridge copy $exe (FileVersion $($ver.FileVersion), ProductVersion $($ver.ProductVersion)); $($signed[1])"
  return $exe
}

function Start-FeedServer {
  $port = 8765
  Set-Content (Join-Path $Feed 'probe.txt') 'ok'
  $p = Start-Process python -ArgumentList @('-m', 'http.server', "$port", '--bind', '127.0.0.1', '--directory', $Feed) `
    -RedirectStandardError (Join-Path $Work 'feed-server.err.log') -RedirectStandardOutput (Join-Path $Work 'feed-server.out.log') `
    -WindowStyle Hidden -PassThru
  $ready = Wait-Until { try { (Invoke-WebRequest "http://127.0.0.1:$port/probe.txt" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } catch { $false } } 30
  if (-not $ready) { throw 'the feed server did not start' }
  $script:FeedServer = $p
  return "http://127.0.0.1:$port"
}

# The setup the feed serves, per case.
function Publish-Setup([object] $native, [string] $mode) {
  $served = Join-Path $Feed 'Yap.App-win-Setup.exe'
  $digest = $native.sha256
  switch ($mode) {
    'real' { Copy-Item $native.path $served -Force }
    'tampered' {
      # One byte flipped mid-file: same size, the release's real digest.
      $bytes = [IO.File]::ReadAllBytes($native.path)
      $i = [int]($bytes.Length / 2)
      $bytes[$i] = $bytes[$i] -bxor 0xFF
      [IO.File]::WriteAllBytes($served, $bytes)
      Log "tampered: flipped byte $i; served sha256 $((Get-FileHash $served -Algorithm SHA256).Hash.ToLowerInvariant()), release digest $digest"
    }
    'unsigned' {
      # The real Setup with its Authenticode signature removed, and the
      # digest of those bytes: only the signer check can refuse it.
      Copy-Item $native.path $served -Force
      $signtool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' | Sort-Object FullName -Descending | Select-Object -First 1
      if (-not $signtool) { throw 'signtool.exe not found' }
      & $signtool.FullName remove /s $served | Out-Host
      if ($LASTEXITCODE -ne 0) { throw 'signtool remove failed' }
      $digest = (Get-FileHash $served -Algorithm SHA256).Hash.ToLowerInvariant()
      Log "unsigned: signature status now $((Get-AuthenticodeSignature $served).Status), sha256 $digest"
    }
  }
  [pscustomobject]@{ path = $served; sha256 = $digest; size = (Get-Item $served).Length }
}

function New-Override([string] $url, [long] $size, [string] $sha256, [bool] $switchOn) {
  $override = [ordered]@{
    release = [ordered]@{
      tag_name   = $OverrideTag
      draft      = $false
      prerelease = $false
      assets     = @([ordered]@{
          name                 = 'Yap.App-win-Setup.exe'
          size                 = $size
          browser_download_url = $url
          digest               = "sha256:$sha256"
        })
    }
    switch  = [ordered]@{ enabled = $switchOn; rolloutPercent = 100 }
  }
  $path = Join-Path $Work 'handoff-release.json'
  $override | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8NoBOM $path
  Copy-Evidence $path 'handoff-release.json'
  return $path
}

function Invoke-Bridge([string] $exe, [string] $override, [string] $label) {
  Copy-Evidence $Marker "markers\native-handoff.before-$label.json"
  $script:RunKeyAtBridge = Test-Path $RunKey
  Log "HKCU Run key present as the bridge starts ($label): $($script:RunKeyAtBridge)"
  $env:YAP_HANDOFF_RELEASE = $override
  Remove-Item Env:YAP_NO_NATIVE_HANDOFF -ErrorAction SilentlyContinue
  Log "bridge ($label): $exe --yap-native-handoff with YAP_HANDOFF_RELEASE=$override"
  $started = Get-Date
  $p = Start-Process -FilePath $exe -ArgumentList '--yap-native-handoff' -WorkingDirectory (Split-Path $exe) -PassThru
  $null = $p.Handle
  $finished = $p.WaitForExit(15 * 60 * 1000)
  Remove-Item Env:YAP_HANDOFF_RELEASE -ErrorAction SilentlyContinue
  if (-not $finished) {
    Check "bridge-$label-finished" $false 'still running after 15 min'
    return $null
  }
  $secs = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
  Log "bridge ($label) exited with $($p.ExitCode) after $secs s"
  Copy-Evidence $Marker "markers\native-handoff.after-$label.json"
  return [pscustomobject]@{ code = $p.ExitCode; seconds = $secs; ended = Get-Date }
}

# ── Assertions ───────────────────────────────────────────────────────────

function Assert-TauriIntact([string] $label) {
  Check "$label-tauri-still-installed" ((Test-Path $TauriRoot) -and (Test-Path "$UninstallKey\Yap")) "root exists=$(Test-Path $TauriRoot), Uninstall\Yap exists=$(Test-Path "$UninstallKey\Yap")"
}

function Assert-NativeAbsent([string] $label) {
  Check "$label-native-not-installed" (-not (Test-Path $NativeExe) -and -not (Test-Path "$UninstallKey\Yap.App")) "current\Yap.exe exists=$(Test-Path $NativeExe), Uninstall\Yap.App exists=$(Test-Path "$UninstallKey\Yap.App")"
}

function Assert-HandedOff([object] $run, [string] $label) {
  Check "$label-exit-0" ($run -and $run.code -eq 0) "exit=$($run.code) after $($run.seconds) s"
  Check "$label-native-exe" (Test-Path $NativeExe) $NativeExe
  if (Test-Path $NativeExe) {
    $signed = Test-SignedByYap $NativeExe
    Check "$label-native-exe-signed" $signed[0] $signed[1]
  }
  $procs = Get-NativeProcesses
  Check "$label-native-running" ($procs.Count -gt 0) "processes from current\Yap.exe: $(@($procs | ForEach-Object ProcessId) -join ',')"
  $m = Read-Json $Marker
  Check "$label-marker-done" ($m -and $m.state -eq 'done' -and $m.version -eq $OverrideVersion) "state=$($m.state) version=$($m.version) attempts=$($m.attempts) lastError=$($m.lastError)"
  Check "$label-log-handed-off" ((Get-HandoffLog) -match 'handed off') 'handoff.log has "handed off"'
}

# After a handoff: the native window loads, the deferred uninstall removes
# the Tauri app, the registrations and the import are as expected.
function Assert-Cutover([object] $run) {
  $appStarted = Wait-Until { (Get-InstallerLog) -match '\[tauri\] app started' } 120
  $sinceExit = [math]::Round(((Get-Date) - $run.ended).TotalSeconds, 1)
  Check 'native-window-loaded' $appStarted "installer.log '[tauri] app started' $(if ($appStarted) { "within $sinceExit s of the bridge's exit" } else { 'not seen in 120 s' })"
  Save-Screenshot '03-native-up'
  if (-not $appStarted) {
    Note 'native never reported its window (yap_tauri_app_started); see logs\yap-native.log and the screenshots'
  }
  $t = Get-Date
  $gone = Wait-Until { -not (Test-Path $TauriRoot) -and -not (Test-Path "$UninstallKey\Yap") } 120
  $took = [math]::Round(((Get-Date) - $t).TotalSeconds, 1)
  Check 'tauri-root-removed' (-not (Test-Path $TauriRoot)) "$TauriRoot exists=$(Test-Path $TauriRoot) ($took s after the window)"
  Check 'tauri-uninstall-key-removed' (-not (Test-Path "$UninstallKey\Yap")) "Uninstall\Yap exists=$(Test-Path "$UninstallKey\Yap")"
  Check 'native-uninstall-key-present' (Test-Path "$UninstallKey\Yap.App") "Uninstall\Yap.App DisplayName=$(Get-RegValue "$UninstallKey\Yap.App" 'DisplayName') DisplayVersion=$(Get-RegValue "$UninstallKey\Yap.App" 'DisplayVersion')"
  $log = Get-InstallerLog
  Check 'tauri-uninstaller-exit-0' ($log -match '\[tauri\] uninstaller exited with 0') (([regex]::Match($log, '\[tauri\] uninstaller[^\r\n]*')).Value)
  Check 'native-still-running-after-uninstall' ((Get-NativeProcesses).Count -gt 0) ''

  $scheme = Get-RegValue $SchemeCommandKey '(default)'
  Check 'yap-scheme-to-native' ("$scheme" -like "*\Yap.App\current\Yap.exe*") "yap:// -> $scheme"
  $runValue = Get-RegValue $RunKey 'Yap'
  $runOk = "$runValue" -like "*\Yap.App\*"
  $runDetail = "Run\Yap -> $runValue (Run key present before seeding: $($script:RunKeyBefore), as the bridge started: $($script:RunKeyAtBridge))"
  if ($Case -eq 'native-installed' -and -not $runOk -and -not $script:RunKeyAtBridge -and $NativeMajor -lt 4) {
    # KNOWN BUG in native releases before 4.0.0: DesktopShell::setAutostart
    # (native/cpp/desktopshell.cpp) opened the Run key with
    # RegOpenKeyExW, which fails when the key does not exist, so open on
    # startup was never written there. 4.0.0 creates the key
    # (RegCreateKeyExW), so from 4.0.0 this is the hard check below.
    Check 'run-value-to-native' $false "KNOWN BUG before 4.0.0 (native setAutostart cannot create a missing Run key): $runDetail" -Soft
  }
  else {
    Check 'run-value-to-native' $runOk $runDetail
  }

  $import = Read-Json (Join-Path $NativeData 'tauri-import.json')
  Check 'import-done' ($import -and $import.done -eq $true) ($import | ConvertTo-Json -Compress)
  if ($import) {
    Check 'import-flags' ($import.desktopSettings -eq $true -and $import.sounds -eq $true -and $import.theme -eq $SeededTheme -and $import.windowState -eq $true) "desktopSettings=$($import.desktopSettings) sounds=$($import.sounds) theme=$($import.theme) windowState=$($import.windowState)"
  }
  $s = Read-Json (Join-Path $NativeData 'desktop-settings.json')
  if ($s) {
    $bad = @()
    foreach ($k in $SeededSettings.Keys) { if ($s.$k -ne $SeededSettings[$k]) { $bad += "$k=$($s.$k)" } }
    if ($s.sounds -ne $false) { $bad += "sounds=$($s.sounds)" }
    if ($s.skipVoiceSwitchConfirm -ne $true) { $bad += "skipVoiceSwitchConfirm=$($s.skipVoiceSwitchConfirm)" }
    if ($s.skipStartCallConfirm -ne $true) { $bad += "skipStartCallConfirm=$($s.skipStartCallConfirm)" }
    $ss = $s.soundSettings
    $want = @{ soundVolume = 42; soundboardVolume = 17; muteDeafen = $false; calls = $false; customSounds = $true }
    foreach ($k in $want.Keys) { if ($ss.$k -ne $want[$k]) { $bad += "soundSettings.$k=$($ss.$k)" } }
    Check 'imported-desktop-settings' ($bad.Count -eq 0) $(if ($bad) { 'mismatch: ' + ($bad -join ', ') } else { 'all seeded values' })
  }
  else {
    Check 'imported-desktop-settings' $false 'Yap.Data\desktop-settings.json missing'
  }
  $a = Read-Json (Join-Path $NativeData 'appearance.json')
  Check 'imported-theme' ($a -and $a.theme -eq $SeededTheme) "appearance.json theme=$($a.theme)"
  $w = Read-Json (Join-Path $NativeData 'window-state.json')
  Check 'imported-window-state' ($w -and $w.x -eq $SeededWindow.x -and $w.y -eq $SeededWindow.y -and $w.width -eq $SeededWindow.width -and $w.height -eq $SeededWindow.height) "window-state.json $($w | ConvertTo-Json -Compress) (seeded physical $($SeededWindow.x),$($SeededWindow.y) $($SeededWindow.width)x$($SeededWindow.height) at 96 DPI)"
  Check 'tauri-data-kept' (Test-Path (Join-Path $TauriData 'desktop-settings.json')) '%APPDATA%\Yap is read, never removed'
}

# ── Evidence at the end ──────────────────────────────────────────────────

function Save-AllEvidence {
  Save-Screenshot '99-final'
  Export-Registry 'final'
  Copy-Evidence $HandoffWork 'yap-native-handoff'
  Copy-Evidence (Join-Path $NativeData 'logs') 'native\logs'
  foreach ($f in 'tauri-import.json', 'tauri-replace.json', 'desktop-settings.json', 'appearance.json', 'window-state.json') {
    Copy-Evidence (Join-Path $NativeData $f) "native\$f"
  }
  Copy-Evidence (Join-Path $env:LOCALAPPDATA 'velopack') 'velopack'
  Copy-Evidence $Marker 'markers\native-handoff.final.json'
  Copy-Evidence (Join-Path $TauriData 'logs') 'tauri\roaming-logs'
  Copy-Evidence (Join-Path $env:LOCALAPPDATA 'com.115jon.yap\logs') 'tauri\app-logs'
  Copy-Evidence (Join-Path $TauriRoot 'logs') 'tauri\root-logs'
  Copy-Evidence (Join-Path $Work 'feed-server.err.log') 'feed-server.log'
  Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, ExecutablePath | Format-Table -AutoSize | Out-String -Width 400 |
  Set-Content (Join-Path $Out 'processes.txt')
  Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, CurrentHorizontalResolution, CurrentVerticalResolution | Format-List | Out-String |
  Set-Content (Join-Path $Out 'video.txt')
}

function Write-Summary {
  $failed = @($script:Checks | Where-Object status -EQ 'fail').Count
  $result = [ordered]@{
    case       = $Case
    os         = "$env:ImageOS $([Environment]::OSVersion.Version)"
    bridgeTag  = $BridgeTag
    nativeTag  = $NativeTag
    updatePath = $script:UpdatePath
    result     = if ($failed) { 'fail' } else { 'pass' }
    checks     = $script:Checks
    notes      = $script:Notes
  }
  $result | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Out 'checks.json')
  if ($env:GITHUB_STEP_SUMMARY) {
    $md = @("### $Case on $env:ImageOS: **$($result.result)**", '', "bridge $BridgeTag, native $NativeTag, update path: $($script:UpdatePath)", '', '| check | result | detail |', '| --- | --- | --- |')
    foreach ($c in $script:Checks) { $md += "| $($c.name) | $($c.status) | $(($c.detail -replace '\|', '/') -replace '[\r\n]+', ' ') |" }
    if ($script:Notes.Count) { $md += ''; $md += ($script:Notes | ForEach-Object { "- $_" }) }
    $md -join "`n" | Add-Content $env:GITHUB_STEP_SUMMARY
  }
  return $failed
}

# ── Main ─────────────────────────────────────────────────────────────────

$script:UpdatePath = 'n/a'
$failed = 1
try {
  Log "case ${Case}: $OldTag -> $BridgeTag, native $NativeTag as $OverrideTag; $([Environment]::OSVersion.VersionString)"
  Get-CimInstance Win32_VideoController | ForEach-Object { Log "video: $($_.Name) $($_.CurrentHorizontalResolution)x$($_.CurrentVerticalResolution)" }
  $native = Get-Asset $NativeTag 'Yap.App-win-Setup.exe'
  $sig = Test-SignedByYap $native.path
  Check 'native-setup-signed' $sig[0] $sig[1]

  $realUpdater = $Case -eq 'happy' -and $BridgeIsLatest
  # The installed Tauri app's own bridge stays off (it asks production
  # GitHub and the real switch, and shares the marker and handoff.log): the
  # apps the installers start inherit this. Invoke-Bridge clears it.
  $env:YAP_NO_NATIVE_HANDOFF = '1'
  Install-OldTauri $realUpdater
  Update-ToBridge $realUpdater
  # native-installed leaves the Tauri app without autostart (no Run value).
  Write-Seeds ($Case -ne 'native-installed')
  Export-Registry 'before-bridge'
  # The Tauri root (and its installer logs) is gone once the cutover works.
  Copy-Evidence (Join-Path $TauriRoot 'logs') 'tauri\root-logs'
  $bridge = New-BridgeCopy
  $base = Start-FeedServer

  switch ($Case) {
    'happy' {
      $served = Publish-Setup $native 'real'
      $override = New-Override "$base/Yap.App-win-Setup.exe" $served.size $served.sha256 $true
      $run = Invoke-Bridge $bridge $override 'first'
      Assert-HandedOff $run 'handoff'
      Check 'setup-downloaded-once' ((Get-FeedRequests) -eq 1) "GET requests for the Setup: $(Get-FeedRequests)"
      Assert-Cutover $run
    }
    'rerun' {
      # The first run downloads from GitHub itself (redirects and all).
      $override = New-Override $native.url $native.size $native.sha256 $true
      $run = Invoke-Bridge $bridge $override 'first'
      Assert-HandedOff $run 'first'
      Assert-Cutover $run
      $before = Read-Json $Marker
      $importBefore = (Get-Item (Join-Path $NativeData 'tauri-import.json') -ErrorAction SilentlyContinue).LastWriteTimeUtc
      $procsBefore = (Get-NativeProcesses).Count
      $logBefore = (Get-InstallerLog).Length
      $setupLog = Join-Path $HandoffWork 'setup.log'
      $setupLogBefore = (Get-Item $setupLog -ErrorAction SilentlyContinue).LastWriteTimeUtc
      # The second run, against the local feed: any download would show.
      $served = Publish-Setup $native 'real'
      $override = New-Override "$base/Yap.App-win-Setup.exe" $served.size $served.sha256 $true
      $hlBefore = (Get-HandoffLog).Length
      $run2 = Invoke-Bridge $bridge $override 'second'
      Assert-HandedOff $run2 'second'
      $tail = (Get-HandoffLog).Substring($hlBefore)
      Check 'second-launched-and-up' ($tail -match 'the native app is up after' -and $tail -match 'handed off') 'this run logged "the native app is up after" and "handed off"'
      Check 'second-skipped-setup' ($tail -match 'installed already; skipping Setup') 'handoff.log: "the native app is installed already; skipping Setup"'
      Check 'second-no-download' ((Get-FeedRequests) -eq 0 -and $tail -notmatch 'downloaded and checked') "GET requests: $(Get-FeedRequests)"
      Check 'second-setup-not-run' (((Get-Item $setupLog -ErrorAction SilentlyContinue).LastWriteTimeUtc) -eq $setupLogBefore -and $tail -notmatch 'running .*--silent') 'setup.log untouched'
      $after = Read-Json $Marker
      Check 'second-marker-stable' ($after.state -eq 'done' -and $after.attempts -eq $before.attempts -and $after.installId -eq $before.installId) "attempts $($before.attempts)->$($after.attempts), installId kept=$($after.installId -eq $before.installId)"
      Start-Sleep -Seconds 5
      Check 'second-single-instance' ((Get-NativeProcesses).Count -le $procsBefore) "processes from current\Yap.exe: $procsBefore -> $((Get-NativeProcesses).Count)"
      Check 'second-import-not-redone' (((Get-Item (Join-Path $NativeData 'tauri-import.json') -ErrorAction SilentlyContinue).LastWriteTimeUtc) -eq $importBefore) 'tauri-import.json untouched'
      Check 'second-no-reinstall-of-tauri' (-not (Test-Path $TauriRoot) -and -not (Test-Path "$UninstallKey\Yap")) ''
      $newLog = (Get-InstallerLog).Substring([math]::Min($logBefore, (Get-InstallerLog).Length))
      Check 'second-no-new-uninstall' ($newLog -notmatch 'uninstalling') "installer.log since: $(($newLog -split "`n" | Where-Object { $_ -match '\[tauri\]' }) -join ' / ')"
    }
    'tampered' {
      $served = Publish-Setup $native 'tampered'
      $override = New-Override "$base/Yap.App-win-Setup.exe" $served.size $served.sha256 $true
      $run = Invoke-Bridge $bridge $override 'tampered'
      Check 'exit-1' ($run -and $run.code -eq 1) "exit=$($run.code)"
      $m = Read-Json $Marker
      Check 'marker-failure-counted' ($m.state -eq 'pending' -and $m.attempts -eq 1 -and "$($m.lastError)" -match 'SHA-256') "state=$($m.state) attempts=$($m.attempts) lastError=$($m.lastError)"
      Check 'log-digest-refused' ((Get-HandoffLog) -match 'SHA-256 does not match') ''
      Check 'no-setup-run' (-not (Test-Path (Join-Path $HandoffWork 'setup.log')) -and (Get-HandoffLog) -notmatch 'running .*--silent') ''
      Check 'no-partial-left' (@(Get-ChildItem $HandoffWork -Filter 'Yap.App-win-Setup-*' -ErrorAction SilentlyContinue).Count -eq 0) (@(Get-ChildItem $HandoffWork -ErrorAction SilentlyContinue | ForEach-Object Name) -join ', ')
      Assert-NativeAbsent 'tampered'
      Assert-TauriIntact 'tampered'
    }
    'unsigned' {
      $served = Publish-Setup $native 'unsigned'
      $override = New-Override "$base/Yap.App-win-Setup.exe" $served.size $served.sha256 $true
      $run = Invoke-Bridge $bridge $override 'unsigned'
      Check 'exit-1' ($run -and $run.code -eq 1) "exit=$($run.code)"
      $m = Read-Json $Marker
      Check 'marker-failure-counted' ($m.state -eq 'pending' -and $m.attempts -eq 1 -and $m.lastError) "state=$($m.state) attempts=$($m.attempts) lastError=$($m.lastError)"
      Check 'log-signature-refused' ("$($m.lastError)" -match 'Authenticode verification failed' -and "$($m.lastError)" -notmatch 'SHA-256' -and (Get-HandoffLog) -notmatch 'Setup verified') (([regex]::Match((Get-HandoffLog), 'attempt failed[^\r\n]*')).Value)
      Check 'no-setup-run' (-not (Test-Path (Join-Path $HandoffWork 'setup.log')) -and (Get-HandoffLog) -notmatch 'running .*--silent') ''
      Assert-NativeAbsent 'unsigned'
      Assert-TauriIntact 'unsigned'
    }
    'switch-off' {
      $served = Publish-Setup $native 'real'
      $override = New-Override "$base/Yap.App-win-Setup.exe" $served.size $served.sha256 $false
      $run = Invoke-Bridge $bridge $override 'switch-off'
      Check 'exit-2' ($run -and $run.code -eq 2) "exit=$($run.code)"
      Check 'log-switch-off' ((Get-HandoffLog) -match 'nothing to hand off to: switch is off' -and (Get-HandoffLog) -notmatch 'is ignored by an installed copy') 'the override was read (not "ignored by an installed copy") and its switch is off'
      Check 'nothing-downloaded' ((Get-FeedRequests) -eq 0 -and @(Get-ChildItem $HandoffWork -Filter 'Yap.App-win-Setup-*' -ErrorAction SilentlyContinue).Count -eq 0) "GET requests: $(Get-FeedRequests)"
      $m = Read-Json $Marker
      Check 'marker-no-attempt' ($m.state -eq 'pending' -and -not $m.lastAttempt -and $m.attempts -eq 0 -and $m.version -ne $OverrideVersion) "state=$($m.state) attempts=$($m.attempts) lastAttempt=$($m.lastAttempt) version=$($m.version)"
      Assert-NativeAbsent 'switch-off'
      Assert-TauriIntact 'switch-off'
    }
    'native-installed' {
      # The native app installed beforehand (silent: it does not start).
      $preLog = Join-Path $Work 'native-preinstall-setup.log'
      $code = Invoke-Installer $native.path @('--silent', '--log', $preLog)
      Copy-Evidence $preLog 'native-preinstall-setup.log'
      Check 'preinstall-native' ($code -eq 0 -and (Test-Path $NativeExe)) "exit $code"
      # In case Setup started it anyway: let that first launch settle
      # (its import), then stop it, so the bridge's launch is the one tested.
      if (Wait-Until { (Get-NativeProcesses).Count -gt 0 } 10) {
        Note 'the native Setup --silent started the app'
        Start-Sleep -Seconds 10
      }
      foreach ($np in Get-NativeProcesses) { Stop-Process -Id $np.ProcessId -Force -ErrorAction SilentlyContinue }
      $served = Publish-Setup $native 'real'
      $override = New-Override "$base/Yap.App-win-Setup.exe" $served.size $served.sha256 $true
      $run = Invoke-Bridge $bridge $override 'native-installed'
      Assert-HandedOff $run 'launch-only'
      Check 'skipped-setup' ((Get-HandoffLog) -match 'installed already; skipping Setup') 'handoff.log: "the native app is installed already; skipping Setup"'
      Check 'no-download' ((Get-FeedRequests) -eq 0 -and (Get-HandoffLog) -notmatch 'downloaded and checked') "GET requests: $(Get-FeedRequests)"
      Check 'no-setup-run' (-not (Test-Path (Join-Path $HandoffWork 'setup.log')) -and (Get-HandoffLog) -notmatch 'running .*--silent') ''
      Assert-Cutover $run
    }
  }
}
catch {
  Check 'harness' $false "$($_.Exception.Message) at $($_.InvocationInfo.PositionMessage)"
}
finally {
  try { Save-AllEvidence } catch { Log "evidence: $($_.Exception.Message)" }
  if ($script:FeedServer) { Stop-Process -Id $script:FeedServer.Id -Force -ErrorAction SilentlyContinue }
  $failed = Write-Summary
}
exit ([int]($failed -gt 0))
