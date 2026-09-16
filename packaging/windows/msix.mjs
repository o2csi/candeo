// Builds the MSIX package Tauri does not build: its bundler makes deb, rpm,
// appimage, msi, nsis, app and dmg, and nothing else. See `docs/design/msix.md`.
//
//   node packaging/windows/msix.mjs [--sign <certificate.pfx> [--password <password>]]
//
// It packages what `tauri build` already produced in `target/release`, so build
// first. The identity is read from the environment, since it belongs to the
// Partner Center account and not to this repository:
//
//   MSIX_IDENTITY_NAME            Identity/Name, as reserved
//   MSIX_PUBLISHER                Identity/Publisher, the CN= string Partner Center gives
//   MSIX_PUBLISHER_DISPLAY_NAME   the publisher as people read it
//
// Without them the package is built for a local test, under a name no Store
// submission would accept, and it has to be signed to install — a self-signed
// certificate is enough, on a machine in developer mode.
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const icons = join(root, 'apps/desktop/src-tauri/icons')
const release = join(root, 'target/release')
const out = join(root, 'target/msix')

// The logos the manifest names, under the names MSIX expects.
const ASSETS = [
  'StoreLogo.png',
  'Square44x44Logo.png',
  'Square71x71Logo.png',
  'Square150x150Logo.png',
]

// Signed with a certificate made for the occasion, whose subject this must
// equal. Windows refuses to register an unsigned package that starts an
// executable, so the OID for unsigned packages is of no use here.
const TEST_IDENTITY = {
  identityName: 'O2CSI.Candeo',
  publisher: 'CN=Candeo test package',
  publisherDisplayName: 'O2CSI',
}

/** The newest `makeappx.exe` or `signtool.exe` of the installed Windows SDK. */
function sdkTool(name) {
  const bin = 'C:/Program Files (x86)/Windows Kits/10/bin'
  const versions = readdirSync(bin)
    .filter((entry) => /^10\./.test(entry))
    .sort()
    .reverse()
  for (const version of versions) {
    const tool = join(bin, version, 'x64', name)
    try {
      readFileSync(tool, { flag: 'r' })
      return tool
    } catch {
      // The SDK is installed without this tool: try the version below.
    }
  }
  throw new Error(`${name} not found: install the Windows SDK`)
}

/** `Identity/Version` takes four numbers, and the Store reserves the last. */
function packageVersion(version) {
  const [major, minor, patch] = version.split('.')
  return `${major}.${minor}.${patch}.0`
}

const argv = process.argv.slice(2)
const option = (name) => {
  const at = argv.indexOf(name)
  return at === -1 ? undefined : argv[at + 1]
}

const config = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'))
const identity = {
  identityName: process.env.MSIX_IDENTITY_NAME ?? TEST_IDENTITY.identityName,
  publisher: process.env.MSIX_PUBLISHER ?? TEST_IDENTITY.publisher,
  publisherDisplayName: process.env.MSIX_PUBLISHER_DISPLAY_NAME ?? TEST_IDENTITY.publisherDisplayName,
  version: packageVersion(config.version),
}
if (!process.env.MSIX_PUBLISHER) {
  console.log(`No MSIX_PUBLISHER: building for a local test, as ${identity.publisher}`)
}

const layout = join(out, 'layout')
rmSync(layout, { recursive: true, force: true })
mkdirSync(join(layout, 'Assets'), { recursive: true })

const exe = `${config.mainBinaryName}.exe`
copyFileSync(join(release, exe), join(layout, exe))
for (const asset of ASSETS) copyFileSync(join(icons, asset), join(layout, 'Assets', asset))

const manifest = readFileSync(join(here, 'AppxManifest.xml'), 'utf8').replaceAll(
  /\{\{(\w+)\}\}/g,
  (_, key) => {
    if (!(key in identity)) throw new Error(`AppxManifest.xml asks for an unknown {{${key}}}`)
    return identity[key]
  },
)
writeFileSync(join(layout, 'AppxManifest.xml'), manifest)

const pkg = join(out, `Candeo_${config.version}_x64.msix`)
execFileSync(sdkTool('makeappx.exe'), ['pack', '/d', layout, '/p', pkg, '/o'], { stdio: 'inherit' })

const certificate = option('--sign')
if (certificate) {
  const password = option('--password')
  execFileSync(
    sdkTool('signtool.exe'),
    [
      'sign',
      '/fd',
      'SHA256',
      '/a',
      '/f',
      certificate,
      ...(password ? ['/p', password] : []),
      pkg,
    ],
    { stdio: 'inherit' },
  )
}

console.log(`\n${pkg}`)
