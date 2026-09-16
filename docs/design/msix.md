# The MSIX package, for the Microsoft Store

Status: **proposed** (#126). What the package holds, what changes for the
application inside one, and how to build and test it.

## Why

- **The warning on first run.** The installers published on GitHub are unsigned,
  and SmartScreen says so. The Store signs the package it distributes, for free,
  at submission. Signing the GitHub installers is a separate question (#145): a
  Store signature covers the package, not the files inside it.
- **Updates come with it**: the Store updates what it installed, so the
  application needs no updater of its own there (#139).
- **Uninstalling takes the package with it**, and the effects people write are
  outside it, in `Documents` (`effects-sources.md`).

Tauri does not build one: its bundler makes `deb`, `rpm`, `appimage`, `msi`,
`nsis`, `app` and `dmg`. The package is built here, from what `tauri build`
already produced.

## 1. What the package holds

```
candeo.exe                  the executable tauri build produced
Assets\                     the logos, from apps/desktop/src-tauri/icons
AppxManifest.xml            packaging/windows/AppxManifest.xml, filled in
```

Nothing else: the application is one executable, and the web view is the
system's.

- **`runFullTrust`**, and no restricted capability. It is a desktop application
  writing to the keyboard through the HID API, as it does outside a package.
  `unvirtualizedResources`, which `effects-sources.md` weighed, is **not** asked
  for: the effects people write live in `Documents`, outside the package.
- **`Identity`** is what Partner Center reserves. `Name`, `Publisher` — the
  `CN=` string of the account, not a certificate someone picks — and
  `PublisherDisplayName` come from there, through the environment
  (`MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER`, `MSIX_PUBLISHER_DISPLAY_NAME`): a
  package whose identity differs from the reservation is refused at submission.
  For this listing, and they are public — every published package carries them:

  ```
  MSIX_IDENTITY_NAME=O2CSI.Candeo
  MSIX_PUBLISHER=CN=3DB35F84-A90B-410A-8375-06E93C7AB6C4
  MSIX_PUBLISHER_DISPLAY_NAME=O2CS&I
  ```

  The display name holds an `&`, which is markup in XML: what the script writes
  into the manifest is escaped.
- **`Version`** takes four numbers, and the Store keeps the last for itself:
  `0.4.0` is packaged as `0.4.0.0`.

## 2. What changes inside a package

| What | Outside | Inside |
|---|---|---|
| The registry | written where it says | written to a store private to the package |
| The installed files | writable | read only |
| Updating | installing over, or an updater (#139) | the Store |
| Uninstalling | offers to keep the data | takes the package's own store with it |

**`AppData` is not redirected**, measured on this package, Windows 11 build
26100: the packaged application appended to the real
`%LOCALAPPDATA%\com.o2csi.candeo\logs`, and the package's `LocalCache` held none
of its files. The redirection older documents describe — every `AppData` write
landing in a store private to the package — did not happen. So `settings.json`,
the cache and the logs are the same files the installed application writes, they
survive uninstalling, and an application installed both ways shares them.

Removing the package took none of it: the logs, `settings.json` and the effects
in `Documents` were all still there afterwards. What uninstalling removes is the
package and its own store, which held nothing here.

It is a floor, not a promise: a future build could redirect again, and the
decision to keep the effects people write in `Documents` (#125) holds either
way, for the reasons it was taken — visible to every program, kept on uninstall,
backed up and synced where people expect.

What follows for the application:

- **The effects people write are already outside**, in `Documents/candeo/effects`
  (#125): visible to every program, kept on uninstall. That decision was taken
  for this package.
- **Launch at login cannot be written**: the `Run` value would go to the
  package's own registry store, where nothing reads it at login. The setting
  says so rather than pretending (`autostart::Refused::Packaged`), and Windows'
  own startup task, which would replace it, is a separate step — it starts the
  application without `--hidden`, which the design forbids.
- **An updater would have nothing to replace**: the files are read only and the
  package is signed. A check must be skipped when packaged (#139), which
  `msix::packaged()` answers.

## 3. Building and testing

```powershell
pnpm --filter @candeo/desktop exec tauri build --bundles nsis   # or --no-bundle
node packaging/windows/msix.mjs
```

Without `MSIX_PUBLISHER`, the package is built under a test identity no
submission would accept. Installing it takes a signature: Windows refuses to
register an unsigned package that starts an executable, whatever developer mode
says, so `-AllowUnsigned` is for packages carrying content only.

```powershell
# Once: a certificate whose subject equals the manifest's Publisher.
$cert = New-SelfSignedCertificate -Type CodeSigningCert `
  -Subject "CN=Candeo test package" -CertStoreLocation "Cert:\CurrentUser\My"
Export-PfxCertificate -Cert $cert -FilePath candeo-test.pfx -Password $password
Export-Certificate -Cert $cert -FilePath candeo-test.cer

# Once, and the only step needing administrator: trusting that certificate.
Import-Certificate -FilePath candeo-test.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople

# Then, at every build:
node packaging/windows/msix.mjs --sign candeo-test.pfx --password <password>
Add-AppxPackage target\msix\Candeo_<version>_x64.msix
```

Installing the signed package needs no administrator, and neither does removing
it: `Get-AppxPackage *Candeo* | Remove-AppxPackage`. The certificate is for this
machine only — the Store signs what it distributes.

## 4. What the certification kit says

The Windows App Certification Kit, run against the package built with the
reserved identity: **PASS**. One optional test reports what it calls blocked
executables — `candeo.exe` naming `CreateProcessW`, `ShellExecuteW` and
`cmd.exe`. They come from opening a folder or the log folder in the file
manager, which is what a full-trust desktop application does; the test is marked
optional and the overall result is not affected.

```powershell
& "C:\Program Files (x86)\Windows Kits\10\App Certification Kit\appcert.exe" `
  test -appxpackagepath target\msix\Candeo_<version>_x64.msix -reportoutputpath report.xml
```

It installs the package to test it, so it needs administrator and a trusted
signature, and the application must not already be running — one instance at a
time is the rule everywhere else too.

## 5. Out of scope

- Submitting from the release workflow: by hand for the first version.
- The startup task that would bring launch at login back inside the package.
- Signing the GitHub installers (#145) and winget (#138).
