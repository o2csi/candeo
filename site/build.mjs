// Builds the site into `site/_site`, which the Pages workflow uploads.
//
//   node site/build.mjs
//
// The download links are filled in from the latest release, so the page names a
// version that exists — a page that says "download 0.4.0" a year later is worse
// than one that says nothing. The privacy policy is rendered from `PRIVACY.md`,
// which stays the only copy of it: a policy that exists twice is a policy that
// disagrees with itself. Code is coloured here, and the landing page's keyboard
// runs its example with the engine's helpers, on the geometry the app draws.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import hljs from 'highlight.js/lib/core'
import bash from 'highlight.js/lib/languages/bash'
import powershell from 'highlight.js/lib/languages/powershell'
import typescript from 'highlight.js/lib/languages/typescript'
import yaml from 'highlight.js/lib/languages/yaml'
import { marked } from 'marked'

hljs.registerLanguage('bash', bash)
hljs.registerLanguage('powershell', powershell)
hljs.registerLanguage('typescript', typescript)
hljs.registerLanguage('yaml', yaml)

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

/**
 * The devices, from the one file that lists them. The landing page shows a
 * short form and `devices.html` the whole of it, both from here: a device
 * described twice is a device described differently.
 */
const devices = JSON.parse(readFileSync(join(here, 'devices.json'), 'utf8'))

/**
 * The supported devices, as a table. Every column is a fact; what a row cannot
 * hold is in the survey it links to, which is the document that has to stay
 * right anyway.
 */
function table(repo) {
  const rows = devices.supported
    .map(
      (device) => `            <tr>
              <td>${device.maker}</td>
              <td>${device.model}</td>
              <td>${device.kind}</td>
              <td>${device.connection}</td>
              <td>${device.lights}</td>
              <td><a href="${repo}/blob/main/${device.protocol}">${device.surveyed}</a></td>
            </tr>`,
    )
    .join('\n')
  return `        <div class="scroller">
          <table class="devices">
            <thead>
              <tr>
                <th>Maker</th>
                <th>Model</th>
                <th>Kind</th>
                <th>Connection</th>
                <th>Lights</th>
                <th>Protocol</th>
              </tr>
            </thead>
            <tbody>
${rows}
            </tbody>
          </table>
        </div>`
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
  repository: repo,
  devices: table(repo),
}

rmSync(out, { recursive: true, force: true })
mkdirSync(join(out, 'live'), { recursive: true })

const entities = { lt: '<', gt: '>', amp: '&', quot: '"', '#39': "'", '#x27': "'" }

/** The text a block says, from the escaped form the page holds it in. */
function unescaped(html, where) {
  if (/<[a-z/]/i.test(html)) {
    throw new Error(`${where}: a code block holds markup, which highlighting would lose`)
  }
  return html.replaceAll(/&(lt|gt|amp|quot|#39|#x27);/g, (_, name) => entities[name])
}

/** The value of one attribute, or `undefined`. */
function attribute(attributes, name) {
  return new RegExp(`\\b${name}="([^"]*)"`).exec(attributes)?.[1]
}

/**
 * Examples to run in the page are written out beside `live/api.mjs`, the
 * engine's own helpers, as `.mjs` so the build can import them too. They must be
 * plain JavaScript already: one that grows a type annotation fails here, when
 * the build imports it, not in a visitor's browser.
 */
const runnable = new Map()

/**
 * Colours the code blocks when the page is built, so the page itself carries no
 * highlighter. TypeScript unless the block says otherwise with `data-lang`;
 * `data-file` names the block above it, `data-run` also writes it out to run.
 */
function highlighted(page, name) {
  const block = /<pre([^>]*)>(?:<code>)?([\s\S]*?)(?:<\/code>)?<\/pre>/g
  const command = /<div class="command"([^>]*)>([\s\S]*?)<\/div>/g
  return page
    .replaceAll(block, (_, attributes, html) => {
      const text = unescaped(html, name)
      const language = attribute(attributes, 'data-lang') ?? 'typescript'
      const file = attribute(attributes, 'data-file')
      const run = attribute(attributes, 'data-run')
      if (run) {
        runnable.set(run, text)
      }
      const code = hljs.highlight(text, { language }).value
      const kept = attributes.replaceAll(/\s*data-(?:lang|file|run)="[^"]*"/g, '')
      const pre = `<pre${kept}><code class="hljs language-${language}">${code}</code></pre>`
      return file ? `<div class="code"><div class="file">${file}</div>${pre}</div>` : pre
    })
    .replaceAll(command, (_, attributes, html) => {
      const language = attribute(attributes, 'data-lang') ?? 'bash'
      const code = hljs.highlight(unescaped(html, name), { language }).value
      return `<div class="command">${code}</div>`
    })
}

/** One page, with what it asks for filled in. */
function render(name) {
  const page = readFileSync(join(here, name), 'utf8').replaceAll(/\{\{(\w+)\}\}/g, (_, key) => {
    if (!(key in values)) {
      throw new Error(`${name} asks for an unknown {{${key}}}`)
    }
    return values[key]
  })
  writeFileSync(join(out, name), highlighted(page, name))
}

/**
 * The keyboard the landing page lights, read from its definition — the one
 * place its geometry is written, and the one the application reads
 * (`crates/candeo-device/devices/`): a copy here would be a second drawing, and
 * would drift without a sound.
 */
function keyboard() {
  const file = 'crates/candeo-device/devices/razer-deathstalker-v2-pro.json'
  const d = JSON.parse(readFileSync(join(root, file), 'utf8'))
  const { rows, cols, items } = d.lights
  const keys = items.map(({ row, col, rect: [x, y, w, h] }) => ({
    // A position is a row after another: what the engine hands an effect.
    index: row * cols + col,
    row,
    col,
    x,
    y,
    w,
    h,
  }))
  // A key per lit position, which the definition's tests check: a count off
  // means the file is not the one this page was written against.
  if (keys.length !== 106) {
    throw new Error(`${file}: ${keys.length} keys, 106 expected`)
  }
  return { name: d.name, rows, cols, frameLen: rows * cols, keys }
}

render('index.html')
render('devices.html')
render('automations.html')
render('signals.html')
render('sound.html')
render('effects.html')

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

// What the landing page runs: the example as written, the engine's helpers, the
// keyboard. Each example is imported and drawn once here, so a page that would
// throw in the browser fails the build instead.
const layout = keyboard()
writeFileSync(join(out, 'live', 'layout.json'), JSON.stringify(layout))
cpSync(join(root, 'apps/desktop/src-tauri/src/runtime/api.js'), join(out, 'live', 'api.mjs'))
for (const [name, text] of runnable) {
  const file = join(out, 'live', `${name}.mjs`)
  writeFileSync(file, text.replace(`from '@candeo/effects-api'`, `from './api.mjs'`))
  const { default: effect } = await import(pathToFileURL(file).href)
  const lit = new Set()
  effect.render({ layout, time: 1, params: {}, frame: { set: (key) => lit.add(key.index) } })
  if (lit.size !== layout.keys.length) {
    throw new Error(`${name}: ${lit.size} keys lit of ${layout.keys.length}`)
  }
}

cpSync(join(here, 'style.css'), join(out, 'style.css'))
cpSync(join(here, 'copy.js'), join(out, 'copy.js'))
cpSync(join(here, 'live.js'), join(out, 'live.js'))
cpSync(join(here, 'filter.js'), join(out, 'filter.js'))
cpSync(join(here, 'assets'), join(out, 'assets'), { recursive: true })

console.log(`${out} — ${values.version}`)
