<#
.SYNOPSIS
Attaches the package the Microsoft Store signed to its GitHub release, then
sends it to winget (#305).

.DESCRIPTION
Run by a maintainer once the Store has certified version X.Y.Z, on a Windows
signed in with the Microsoft 365 account of the Partner Center tenant:

  pwsh packaging/windows/store-package.ps1 -Version X.Y.Z

`winget download` takes the package from the Store only for an Entra ID user
account, which it reads from the Windows session without asking; it has no way
in for an application's credentials, so no runner can do this step. The app
must allow disconnected (offline) licensing in Partner Center, or the Store
refuses the download.

The package is checked (version, Microsoft Marketplace signature), attached as
`Candeo_X.Y.Z_x64.msix`, added to the release's `SHA256SUMS`, and `winget.yml`
is started for the tag. Run again, it replaces what it attached. Run before the
certification, it attaches nothing and exits with code 2, which store-watch.ps1
tells apart from a failure.
#>
param(
  [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
  # The Store's product id, public on Candeo's Store page.
  [string]$ProductId = '9MXFM8QT1X7P',
  # Attaches the package without starting winget.yml.
  [switch]$SkipWinget,
  # Whether winget may open a sign-in window: never, from a scheduled task.
  [ValidateSet('silent', 'silentPreferred', 'interactive')][string]$Authentication = 'silentPreferred'
)

$ErrorActionPreference = 'Stop'
$tag = "v$Version"
$name = "Candeo_${Version}_x64.msix"
$work = Join-Path ([IO.Path]::GetTempPath()) "candeo-store-$Version"
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory $work | Out-Null

# The release must be public: the package joins what it already serves.
$draft = gh release view $tag --json isDraft --jq .isDraft
if ($LASTEXITCODE -ne 0) { throw "No release $tag." }
if ($draft -eq 'true') { throw "$tag is still a draft: publish it first." }

winget download --id $ProductId -s msstore --skip-license -a x64 -d $work `
  --authentication-mode $Authentication `
  --accept-source-agreements --accept-package-agreements | Out-Host
if ($LASTEXITCODE -ne 0) { throw "winget download failed ($LASTEXITCODE)." }

# The Store serves the version it last published: until $Version is certified,
# that is the one before.
$package = Get-ChildItem $work -Filter '*.msix' | Select-Object -First 1
if (-not $package) { throw 'The Store sent no MSIX package.' }
if ($package.Name -notlike "*_$Version.0_*") {
  Write-Host "The Store serves $($package.Name), not $Version yet: wait for the certification."
  exit 2
}

$signature = Get-AuthenticodeSignature $package.FullName
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Issuer -notlike 'CN=Microsoft Marketplace CA*') {
  throw "$($package.Name) is not signed by the Microsoft Store: $($signature.Status), $($signature.SignerCertificate.Issuer)."
}

$file = Join-Path $work $name
Move-Item $package.FullName $file
gh release upload $tag $file --clobber
if ($LASTEXITCODE -ne 0) { throw 'The upload failed.' }

# SHA256SUMS gains the package's line, or has it replaced.
gh release download $tag -p SHA256SUMS -D $work --clobber
if ($LASTEXITCODE -ne 0) { throw "The release $tag has no SHA256SUMS." }
$sums = Join-Path $work 'SHA256SUMS'
$hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
$lines = @(Get-Content $sums | Where-Object { $_ -and $_ -notmatch "\s$([regex]::Escape($name))$" })
$lines += "$hash  $name"
# Written as sha256sum writes it: LF, and a last newline.
[IO.File]::WriteAllText($sums, ($lines -join "`n") + "`n")
gh release upload $tag $sums --clobber
if ($LASTEXITCODE -ne 0) { throw 'SHA256SUMS was not replaced.' }
Write-Host "$name attached to $tag, $hash."

if (-not $SkipWinget) {
  gh workflow run winget.yml -f tag=$tag
  if ($LASTEXITCODE -ne 0) { throw 'winget.yml did not start.' }
  Write-Host "winget.yml started for $tag."
}
Remove-Item $work -Recurse -Force
