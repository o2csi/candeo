// Renders the Windows Package Manager manifests for a published release (#138).
//
//   node packaging/winget/winget.mjs 0.4.0
//
// What winget installs is the package the Microsoft Store signed, attached to
// the release once certified (#305). Its checksum is read from the release's
// `SHA256SUMS`, so the manifest describes a file that exists with the hash the
// release declares; the package is downloaded only for the hash of its
// signature, which `SHA256SUMS` cannot give, and checked against that line.
//
// It writes `target/winget/manifests/o/O2CSI/Candeo/<version>/`, the folder
// layout winget-pkgs expects. Check it, then submit it:
//
//   winget validate --manifest target\winget\manifests\o\O2CSI\Candeo\<version>
//
// Submitting is copying that folder into a fork of microsoft/winget-pkgs and
// opening a pull request there.
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { signatureSha256 } from './signature.mjs'

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

/**
 * The project's page, which the repository decides: `owner/name` on GitHub is
 * served at `owner.github.io/name`. The manifest points at it rather than at the
 * code, since that is the page someone lands on.
 */
function site() {
  const path = repository().replace('https://github.com/', '')
  const [owner, name] = path.split('/')
  return `https://${owner.toLowerCase()}.github.io/${name}/`
}

/**
 * The publisher's page: the owner of the repository on GitHub. It answers as
 * long as the repository does, which winget's URL check requires of every
 * address in the manifest — the publisher's own site did not, the day it was
 * submitted.
 */
function publisher() {
  const [owner] = repository().replace('https://github.com/', '').split('/')
  return `https://github.com/${owner}`
}

/** The package's signature hash, from the file the release serves, checked against its line. */
async function signature(name, declared) {
  const url = `${repository()}/releases/download/v${version}/${name}`
  const answer = await fetch(url)
  if (!answer.ok) {
    throw new Error(`${url} answered ${answer.status}`)
  }
  const msix = Buffer.from(await answer.arrayBuffer())
  const hash = createHash('sha256').update(msix).digest('hex').toUpperCase()
  if (hash !== declared) {
    throw new Error(`${name} hashes to ${hash}, SHA256SUMS says ${declared}`)
  }
  return signatureSha256(msix)
}

const msix = `Candeo_${version}_x64.msix`
const values = {
  version,
  repository: repository(),
  site: site(),
  publisher: publisher(),
  date: await releaseDate(version),
  sha256_msix: named(msix),
  signature_msix: await signature(msix, named(msix)),
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
