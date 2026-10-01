<#
.SYNOPSIS
Runs store-package.ps1 and winget.yml for the latest release, as soon as each
can do something (#305).

.DESCRIPTION
Meant for a scheduled task on the maintainer's Windows that runs
store-package.ps1, signed in as that script needs. Registered once, and again
after changing either script:

  pwsh packaging/windows/store-watch.ps1 -Register

The task runs once a day, or as soon as the computer is on after missing it.
Each run looks at the latest release:

- without its signed package, it runs store-package.ps1, which attaches nothing
  while the Store still serves the previous version;
- with it, once winget lists O2CSI.Candeo and the repository holds a
  WINGET_TOKEN secret, it starts winget.yml for a version winget does not have,
  once: a run that fails, or a pull request a moderator refuses, is for a
  person to follow.

What it does, and why it waits, goes to store-watch.log next to the copies, a
line each time that changes. A failure also shows a notification, at every run
until it is fixed.
#>
param(
  # Copies both scripts out of the checkout and registers the task to run them.
  [switch]$Register,
  [string]$Repository = 'o2csi/candeo'
)

$ErrorActionPreference = 'Stop'
$task = 'Candeo store watch'

if ($Register) {
  # The task runs copies: a checkout changes branch, and would run whatever
  # version of the scripts that branch holds, or none.
  $copies = Join-Path $env:LOCALAPPDATA 'candeo-store-watch'
  New-Item -ItemType Directory -Force $copies | Out-Null
  Copy-Item (Join-Path $PSScriptRoot 'store-watch.ps1'), (Join-Path $PSScriptRoot 'store-package.ps1') $copies -Force

  # The App Execution Alias, which survives the updates of PowerShell from the
  # Store; conhost --headless keeps a console window from showing at each run.
  $pwsh = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\pwsh.exe'
  $script = Join-Path $copies 'store-watch.ps1'
  $action = New-ScheduledTaskAction -Execute 'conhost.exe' -WorkingDirectory $copies `
    -Argument "--headless `"$pwsh`" -NoProfile -NonInteractive -File `"$script`" -Repository $Repository"
  $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
  # The Store certifies within days, and winget-pkgs reviews within weeks.
  $daily = New-ScheduledTaskTrigger -Daily -At '12:00'
  $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 30) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  # In the session: winget takes the Store's account from it, gh its sign-in.
  $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
  Register-ScheduledTask -TaskName $task -Action $action -Trigger $daily `
    -Settings $settings -Principal $principal -Force | Out-Null
  Write-Host "'$task' registered: it runs $script."
  return
}

$env:GH_REPO = $Repository
$log = Join-Path $PSScriptRoot 'store-watch.log'

function Say([string]$text) {
  Write-Host $text
  # A wait lasts weeks: the log keeps what changed, not every run.
  $last = Get-Content $log -Tail 1 -ErrorAction SilentlyContinue
  if ($last -and ($last -split '  ', 2)[1] -eq $text) { return }
  "$(Get-Date -Format s)  $text" | Add-Content $log
}

function Alert([string]$text) {
  Say $text
  # PowerShell 7 no longer reaches WinRT; Windows PowerShell shows the toast.
  $body = [Security.SecurityElement]::Escape($text)
  $toast = @"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom, ContentType = WindowsRuntime] | Out-Null
`$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
`$xml.LoadXml('<toast><visual><binding template="ToastGeneric"><text>$task</text><text>$body</text></binding></visual></toast>')
`$app = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe'
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier(`$app).Show([Windows.UI.Notifications.ToastNotification]::new(`$xml))
"@
  powershell.exe -NoProfile -NonInteractive -EncodedCommand ([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($toast)))
}

function Exists([string]$path) {
  gh api $path --silent 2>$null
  $LASTEXITCODE -eq 0
}

try {
  $tag = gh release view --json tagName --jq .tagName
  if ($LASTEXITCODE -ne 0) { throw 'gh could not read the latest release.' }
  $version = $tag.TrimStart('v')
  $name = "Candeo_${version}_x64.msix"

  $assets = gh release view $tag --json assets --jq '.assets[].name'
  if ($assets -notcontains $name) {
    & (Join-Path $PSScriptRoot 'store-package.ps1') -Version $version -SkipWinget -Authentication silent
    if ($LASTEXITCODE -eq 2) {
      Say "${tag}: the Store has not certified it yet."
      return
    }
    Say "${tag}: $name attached."
  }

  $winget = 'repos/microsoft/winget-pkgs/contents/manifests/o/O2CSI/Candeo'
  if (-not (Exists $winget)) {
    Say "${tag}: winget does not list O2CSI.Candeo yet."
    return
  }
  if (Exists "$winget/$version") { return }
  if ((gh secret list --json name --jq '.[].name') -notcontains 'WINGET_TOKEN') {
    Say "${tag}: no WINGET_TOKEN secret, nothing is sent to winget."
    return
  }
  $runs = gh run list --workflow winget.yml --limit 100 --json displayTitle --jq '.[].displayTitle'
  if ($runs -contains "winget $tag") { return }
  gh workflow run winget.yml -f tag=$tag
  if ($LASTEXITCODE -ne 0) { throw 'winget.yml did not start.' }
  Say "${tag}: winget.yml started."
} catch {
  Alert "Failed: $($_.Exception.Message)"
  exit 1
}
