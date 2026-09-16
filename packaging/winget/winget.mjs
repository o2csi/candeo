// Renders the Windows Package Manager manifests for a published release (#138).
//
//   node packaging/winget/winget.mjs 0.4.0
//
// The checksums are read from the `SHA256SUMS` published with the release, so
// the manifests describe files that exist, with the hashes the release itself
// declares — never a file downloaded again and hashed here.
//
// It writes `target/winget/manifests/o/O2CSI/Candeo/<version>/`, the folder
// layout winget-pkgs expects. Check it, then submit it:
//
//   winget validate --manifest target\winget\manifests\o\O2CSI\Candeo\<version>
//
// Submitting is copying that folder into a fork of microsoft/winget-pkgs and
// opening a pull request there.
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')

/** The repository, from the workspace manifest: it names itself once. */
function repository() {
  const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8')
  const found = /^repository\s*=\s*"([^"]+)"/m.exec(cargo)
  if (!found) {
    throw new Error('Cargo.toml declares no repository')
  }
  return found[1].replace(/\/$/, '')
}

/** The files published with a release, by name, from its `SHA256SUMS`. */
async function checksums(version) {
  const url = `${repository()}/releases/download/v${version}/SHA256SUMS`
  const answer = await fetch(url)
  if (!answer.ok) {
    throw new Error(`${url} answered ${answer.status}: is v${version} published?`)
  }
  const sums = new Map()
  for (const line of (await answer.text()).split('\n')) {
    const [hash, name] = line.trim().split(/\s+/)
    if (hash && name) {
      sums.set(name, hash.toUpperCase())
    }
  }
  return sums
}

/** The day the release was published, as the manifest wants it. */
async function releaseDate(version) {
  const api = repository().replace('https://github.com/', 'https://api.github.com/repos/')
  const answer = await fetch(`${api}/releases/tags/v${version}`)
  if (!answer.ok) {
    throw new Error(`no release v${version}: GitHub answered ${answer.status}`)
  }
  const { published_at: published } = await answer.json()
  return published.slice(0, 10)
}

const version = process.argv[2]
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('usage: node packaging/winget/winget.mjs <version>, as 0.4.0')
  process.exit(2)
}

const sums = await checksums(version)
const named = (name) => {
  const hash = sums.get(name)
  if (!hash) {
    throw new Error(`SHA256SUMS of v${version} holds no ${name}`)
  }
  return hash
}

const values = {
  version,
  repository: repository(),
  date: await releaseDate(version),
  sha256_setup: named(`candeo_${version}_x64-setup.exe`),
  sha256_msi: named(`candeo_${version}_x64_en-US.msi`),
}

const out = join(root, 'target/winget/manifests/o/O2CSI/Candeo', version)
mkdirSync(out, { recursive: true })
for (const name of readdirSync(here).filter((file) => file.endsWith('.yaml'))) {
  const rendered = readFileSync(join(here, name), 'utf8').replaceAll(/\{\{(\w+)\}\}/g, (_, key) => {
    if (!(key in values)) {
      throw new Error(`${name} asks for an unknown {{${key}}}`)
    }
    return values[key]
  })
  writeFileSync(join(out, name), rendered)
}

console.log(out)
