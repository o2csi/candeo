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
  (`MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER`, `MSIX_PUBLISHER_DISPLAY_NAME`), never
  from this repository: they belong to an account, and a package whose identity
  differs from the reservation is refused at submission.
- **`Version`** takes four numbers, and the Store keeps the last for itself:
  `0.4.0` is packaged as `0.4.0.0`.

## 2. What changes inside a package

| What | Outside | Inside |
|---|---|---|
| `AppData` and the registry | written where they say | written to a store private to the package, merged only for the application itself |
| The installed files | writable | read only |
| Updating | installing over, or an updater (#139) | the Store |
| Uninstalling | offers to keep the data | takes the package's data with it |

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

## 4. Out of scope

- Submitting from the release workflow: by hand for the first version.
- The startup task that would bring launch at login back inside the package.
- Signing the GitHub installers (#145) and winget (#138).
