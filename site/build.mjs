// Builds the site into `site/_site`, which the Pages workflow uploads.
//
//   node site/build.mjs
//
// Two things happen here. The download links are filled in from the latest
// release, so the page names a version that exists — a page that says "download
// 0.4.0" a year later is worse than one that says nothing. And the privacy
// policy is rendered from `PRIVACY.md`, which stays the only copy of it: a
// policy that exists twice is a policy that disagrees with itself.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { marked } from 'marked'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '..')
const out = join(here, '_site')

/** The repository, from the workspace manifest, as everything else reads it. */
function repository() {
  const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8')
  const found = /^repository\s*=\s*"([^"]+)"/m.exec(cargo)
  if (!found) {
    throw new Error('Cargo.toml declares no repository')
  }
  return found[1].replace(/\/$/, '')
}

/**
 * The latest release, or `null` when GitHub cannot be asked — a build without
 * network still produces a page, pointing at the releases list.
 */
async function latest(repo) {
  const api = repo.replace('https://github.com/', 'https://api.github.com/repos/')
  try {
    const answer = await fetch(`${api}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' },
    })
    if (!answer.ok) {
      console.warn(`no latest release: GitHub answered ${answer.status}`)
      return null
    }
    const { tag_name: tag } = await answer.json()
    return typeof tag === 'string' ? tag.replace(/^v/, '') : null
  } catch (e) {
    console.warn(`no latest release: ${e}`)
    return null
  }
}

const repo = repository()
const version = await latest(repo)
const releases = `${repo}/releases`

const values = {
  version: version ?? 'the latest version',
  releaseUrl: version ? `${releases}/tag/v${version}` : releases,
  setupUrl: version
    ? `${releases}/download/v${version}/candeo_${version}_x64-setup.exe`
    : `${releases}/latest`,
}

rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })

const page = readFileSync(join(here, 'index.html'), 'utf8').replaceAll(
  /\{\{(\w+)\}\}/g,
  (_, key) => {
    if (!(key in values)) {
      throw new Error(`index.html asks for an unknown {{${key}}}`)
    }
    return values[key]
  },
)
writeFileSync(join(out, 'index.html'), page)

// The policy, rendered from the one copy of it.
const policy = marked.parse(readFileSync(join(root, 'PRIVACY.md'), 'utf8'))
writeFileSync(
  join(out, 'privacy.html'),
  `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Candeo — privacy policy</title>
    <meta name="description" content="Candeo collects nothing, sends nothing, and has no accounts." />
    <link rel="icon" href="assets/icon.png" />
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link
      rel="stylesheet"
      href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap"
    />
    <link rel="stylesheet" href="style.css" />
  </head>
  <body>
    <main class="policy">
      <a class="back" href="./">← Candeo</a>
${policy}
    </main>
  </body>
</html>
`,
)

cpSync(join(here, 'style.css'), join(out, 'style.css'))
cpSync(join(here, 'assets'), join(out, 'assets'), { recursive: true })

console.log(`${out} — ${values.version}`)
