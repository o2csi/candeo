// Takes the Store's screenshots, and the site's gallery, from the running app.
//
//   pnpm tauri dev, started with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
//   node packaging/store/screenshots/capture.mjs [--gallery]
//
// Drives the window over the WebView2 debugging protocol: it sets the viewport,
// the theme and each language in turn, clicks through the screens, and writes
// `<locale>/NN-name.png` beside this file (and `site/assets/gallery.png` with
// --gallery). What it shows is what this computer has: the devices plugged in
// and their effects. The language the app had is restored; so is the split of a
// device and the folding of *Yours*, which is folded so that no personal effect
// shows. Look at every image before committing it: no serial number, path or
// name may appear.
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = join(HERE, '..', '..', '..')
const STORE = { width: 1440, height: 900, theme: 'dark' }
const GALLERY = { width: 1280, height: 1300, theme: 'light' }

// The labels clicked, by language: the interface's own, from the catalogs.
const LABELS = {
  en: { effects: 'Effects', automations: 'Automations', devices: 'Devices', games: 'Games', settings: 'Settings', split: 'Split into regions', all: 'All', plugged: 'Plugged in', view: 'View code', back: 'Back', yours: 'Yours', examples: 'Add the examples', remove: 'Delete' },
  fr: { effects: 'Effets', automations: 'Automatisations', devices: 'Périphériques', games: 'Jeux', settings: 'Réglages', split: 'Scinder en régions', all: 'Tous', plugged: 'Branchés', view: 'Voir le code', back: 'Retour', yours: 'À vous', examples: 'Ajouter les exemples', remove: 'Supprimer' },
}
const LOCALES = { en: 'en-us', fr: 'fr-fr' }

async function connect() {
  const pages = await (await fetch('http://127.0.0.1:9222/json')).json()
  const page = pages.find((p) => p.type === 'page')
  if (!page) throw new Error('no page on port 9222: start the app with remote debugging')
  const ws = new WebSocket(page.webSocketDebuggerUrl)
  const pending = new Map()
  let id = 0
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data)
    pending.get(message.id)?.(message)
    pending.delete(message.id)
  }
  await new Promise((resolve) => (ws.onopen = resolve))
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      pending.set(++id, resolve)
      ws.send(JSON.stringify({ id, method, params }))
    })
  const run = async (expression) =>
    (await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result?.result?.value
  return { ws, send, run }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

function actions({ run, send }, L) {
  const js = (body) => run(`(() => { ${body} })()`)
  const click = async (text, wait = 800) => {
    const done = await js(`
      const t = ${JSON.stringify(text)}
      const all = [...document.querySelectorAll('button, a, label, [role=button], [role=tab], li')]
      const el = all.find(e => e.textContent.trim() === t) || all.find(e => e.textContent.trim().startsWith(t))
      if (!el) return false
      el.scrollIntoView({ block: 'center' }); el.click(); return true`)
    await sleep(wait)
    return done
  }
  return {
    click,
    nav: async (tab) => {
      await js(`const e = [...document.querySelectorAll('header a, header button, nav a, nav button')].find(e => e.textContent.trim() === ${JSON.stringify(tab)}); e && e.click()`)
      await sleep(1000)
    },
    device: async (pattern) => {
      await js(`const b = [...document.querySelectorAll('button.entry')].find(b => ${pattern}.test(b.textContent.trim())); b && b.click()`)
      await sleep(800)
    },
    split: async (want) => {
      const was = await js(`
        const l = [...document.querySelectorAll('label')].find(e => e.textContent.includes(${JSON.stringify(L.split)}))
        const i = l && l.querySelector('input'); if (!i) return null
        const was = i.checked; if (was !== ${want}) l.click(); return was`)
      await sleep(2000)
      return was
    },
    yours: async (open) => {
      const was = await js(`
        const b = [...document.querySelectorAll('button.group')].find(b => b.textContent.trim().toLowerCase() === ${JSON.stringify(L.yours.toLowerCase())})
        if (!b) return null
        const was = b.getAttribute('aria-expanded') === 'true'; if (was !== ${open}) b.click(); return was`)
      await sleep(400)
      return was
    },
    // The Games tab shows only while Valve's integration is on (Settings).
    valve: async (on) => {
      const was = await js(`const l = [...document.querySelectorAll('label')].find(e => /Valve/.test(e.textContent))
        const i = l && l.querySelector('input'); if (!i) return null
        const was = i.checked; if (was !== ${on}) l.click(); return was`)
      await sleep(1500)
      return was
    },
    language: async (code) => {
      const was = await js(`
        const s = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.value === 'fr'))
        if (!s) return null
        const was = s.value; s.value = ${JSON.stringify(code)}; s.dispatchEvent(new Event('change', { bubbles: true })); return was`)
      await sleep(1500)
      return was
    },
    shot: async (path) => {
      await sleep(600)
      const { result } = await send('Page.captureScreenshot', { format: 'png' })
      mkdirSync(dirname(path), { recursive: true })
      writeFileSync(path, Buffer.from(result.data, 'base64'))
      console.log('wrote', path)
    },
  }
}

async function view(cdp, { width, height, theme }) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] })
  await sleep(800)
}

async function storeSet(cdp, lang) {
  const L = LABELS[lang]
  const a = actions(cdp, L)
  const out = (name) => join(HERE, LOCALES[lang], name)
  await a.click(L.back)
  await a.nav(L.devices)
  const wasSplit = await a.split(false)
  await a.nav(L.effects)
  const wasOpen = await a.yours(false)
  await a.device(/^Razer DeathStalker/)
  await a.click('Diagonal wave', 2500)
  await a.shot(out('01-effects.png'))
  await a.device(/^Alienware m18 R1(?!\s*zones)/)
  await a.click('Radial wave', 2500)
  await a.shot(out('02-laptop-keyboard.png'))
  await a.nav(L.devices)
  await a.split(true)
  await a.nav(L.effects)
  await a.yours(false)
  await cdp.run(`(() => { const b = [...document.querySelectorAll('button')].find(b => /^\\s*Logo/.test(b.textContent)); b && b.click() })()`)
  await sleep(1000)
  // A region shows its own list, unfolded again.
  await a.yours(false)
  await a.click('Color wheel', 2500)
  await a.shot(out('03-regions.png'))
  await a.nav(L.devices)
  await a.split(wasSplit ?? false)
  await a.nav(L.automations)
  // With no rule, the examples are added for the picture, then deleted again.
  const examples = await a.click(L.examples, 1500)
  await a.shot(out('04-automations.png'))
  if (examples) for (let i = 0; i < 20 && (await a.click(L.remove, 600)); i++);
  await a.nav(L.games)
  await a.shot(out('05-games.png'))
  await a.nav(L.devices)
  await a.click(L.all, 1500)
  await a.shot(out('06-devices.png'))
  await a.click(L.plugged)
  await a.nav(L.effects)
  await a.yours(false)
  await a.device(/^Razer DeathStalker/)
  await a.click('Diagonal wave')
  await a.click(L.view, 3000)
  await a.shot(out('07-editor.png'))
  await a.click(L.back)
  if (wasOpen !== null) await a.yours(wasOpen)
}

/** Settings, by its name in either language. */
async function toSettings(cdp) {
  await cdp.run(`(() => { const e = [...document.querySelectorAll('header a, header button, nav a, nav button')].find(e => ['Settings', 'Réglages'].includes(e.textContent.trim())); e && e.click() })()`)
  await sleep(1200)
}

async function main() {
  const gallery = process.argv.includes('--gallery')
  const cdp = await connect()
  const en = actions(cdp, LABELS.en)
  // The app's language and Valve's integration, whatever they are, to come back to them.
  await en.click(LABELS.en.back)
  await toSettings(cdp)
  const was = await en.language('en')
  if (was === null) throw new Error('the language setting was not found in Settings')
  const valve = await en.valve(true)
  try {
    if (gallery) {
      await view(cdp, GALLERY)
      const L = LABELS.en
      await en.nav(L.effects)
      const wasOpen = await en.yours(false)
      await en.device(/^Razer DeathStalker/)
      await en.click('Diagonal wave', 2500)
      await en.shot(join(ROOT, 'site', 'assets', 'gallery.png'))
      if (wasOpen !== null) await en.yours(wasOpen)
    }
    await view(cdp, STORE)
    for (const lang of ['en', 'fr']) {
      if (lang !== 'en') {
        await toSettings(cdp)
        await en.language(lang)
      }
      await storeSet(cdp, lang)
      if (lang !== 'en') {
        await toSettings(cdp)
        await en.language('en')
      }
    }
  } finally {
    await toSettings(cdp)
    if (valve !== null) await en.valve(valve)
    await en.language(was)
    await cdp.send('Emulation.clearDeviceMetricsOverride')
    await cdp.send('Emulation.setEmulatedMedia', { features: [] })
    cdp.ws.close()
  }
}

main().catch((error) => {
  console.error(error.message)
  process.exit(1)
})
