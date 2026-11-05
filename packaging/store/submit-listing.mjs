// Sends the listings and the screenshots of this folder to the Microsoft Store
// between releases, without a package (store-listing.yml). v0.22.0 shipped with
// a listing that did not name its features, and only Partner Center took
// screenshots: this does both from the repository.
//
//   node packaging/store/submit-listing.mjs <tag> [--dry-run]
//
// Through Partner Center's submission API rather than `msstore`, which uploads
// no image. A submission is created from the last published one, the listings
// and the screenshots written into it, the images sent as one ZIP to the
// submission's upload URL, then the submission is committed and followed until
// certification starts. With --dry-run, everything but the commit, then the
// draft is deleted. A submission this script created and could not commit is
// deleted too, so that none is left half done.
//
// Reads TENANT_ID, CLIENT_ID, CLIENT_SECRET and PRODUCT_ID from the environment.
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { crc32 } from 'node:zlib'

import { applyListings, languageOf, newsAdded, previousRelease, readListings, whatsNew } from './listing.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const SCREENSHOTS = join(HERE, 'screenshots')
const API = 'https://manage.devcenter.microsoft.com/v1.0/my/applications'
// Partner Center takes up to ten desktop screenshots per language.
const SCREENSHOTS_MAX = 10

/** The key of `object` equal to `name` ignoring case: the API answers in camelCase. */
function keyOf(object, name) {
  return Object.keys(object ?? {}).find((key) => key.toLowerCase() === name.toLowerCase())
}

/** The screenshots kept here, by Store locale (`en-us`), in the order they are shown. */
export function readScreenshots(folder = SCREENSHOTS) {
  if (!existsSync(folder)) return {}
  return Object.fromEntries(
    readdirSync(folder, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && /^[a-z]{2}-[a-z]{2}$/.test(entry.name))
      .map((entry) => {
        const files = readdirSync(join(folder, entry.name)).filter((name) => /\.png$/.test(name)).sort()
        if (files.length > SCREENSHOTS_MAX) {
          throw new Error(`${entry.name}: ${files.length} screenshots, the Store takes ${SCREENSHOTS_MAX}`)
        }
        return [entry.name, files.map((name) => join(folder, entry.name, name))]
      })
      .filter(([, files]) => files.length > 0),
  )
}

/**
 * The submission with each language's screenshots replaced: the ones it has
 * marked for deletion, the new ones pending upload, named as they will sit in
 * the ZIP. Flat names (`en-us-01-effects.png`) rather than folders: the API's
 * examples disagree on the separator, and a flat name has none. Returns the
 * entries the ZIP must hold, and the locales the submission does not list.
 */
export function applyScreenshots(submission, byLocale) {
  const updated = structuredClone(submission)
  const entries = []
  const skipped = []
  const byLanguage = updated[keyOf(updated, 'listings')] ?? {}
  for (const [locale, paths] of Object.entries(byLocale)) {
    const localeKey = keyOf(byLanguage, locale)
    const base = localeKey && byLanguage[localeKey][keyOf(byLanguage[localeKey], 'baseListing')]
    if (!base) {
      skipped.push(locale)
      continue
    }
    const imagesKey = keyOf(base, 'images') ?? 'images'
    const kept = (base[imagesKey] ?? []).map((image) =>
      image[keyOf(image, 'imageType')] === 'Screenshot'
        ? { ...image, [keyOf(image, 'fileStatus') ?? 'fileStatus']: 'PendingDelete' }
        : image,
    )
    const added = paths.map((path) => {
      const name = `${locale}-${path.split(/[\\/]/).pop()}`
      entries.push({ name, path })
      return { fileName: name, fileStatus: 'PendingUpload', imageType: 'Screenshot', description: '' }
    })
    base[imagesKey] = [...kept, ...added]
  }
  return { submission: updated, entries, skipped }
}

/**
 * A ZIP of `files` ({ name, data }), stored without compression: PNG files do
 * not shrink, and the format stays a few lines that a test can read back.
 */
export function storedZip(files) {
  const parts = []
  const directory = []
  let offset = 0
  for (const { name, data } of files) {
    const nameBytes = Buffer.from(name)
    const crc = crc32(data)
    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt16LE(0x21, 12) // 1980-01-01: no date is part of what is sent
    local.writeUInt32LE(crc, 14)
    local.writeUInt32LE(data.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(nameBytes.length, 26)
    parts.push(local, nameBytes, data)

    const central = Buffer.alloc(46)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE(20, 4)
    central.writeUInt16LE(20, 6)
    central.writeUInt16LE(0x21, 14)
    central.writeUInt32LE(crc, 16)
    central.writeUInt32LE(data.length, 20)
    central.writeUInt32LE(data.length, 24)
    central.writeUInt16LE(nameBytes.length, 28)
    central.writeUInt32LE(offset, 42)
    directory.push(central, nameBytes)
    offset += 30 + nameBytes.length + data.length
  }
  const centralBytes = Buffer.concat(directory)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(files.length, 8)
  end.writeUInt16LE(files.length, 10)
  end.writeUInt32LE(centralBytes.length, 12)
  end.writeUInt32LE(offset, 16)
  return Buffer.concat([...parts, centralBytes, end])
}

/** The listings with the What's new `tag` sent, as the release wrote them. */
export function listingsAsReleased(tag) {
  const listings = readListings()
  const languages = listings.map(languageOf)
  const since = previousRelease(tag.slice(1), tag)
  const added = newsAdded(since, tag)
  const fragments = Object.fromEntries(added.map((path) => [path, readFileSync(join(HERE, path), 'utf8')]))
  const news = added.length > 0 ? whatsNew(fragments, languages, tag.slice(1)) : null
  return listings.map((listing) =>
    news ? { ...listing, fields: { ...listing.fields, releaseNotes: news[languageOf(listing)] } } : listing,
  )
}

async function token({ TENANT_ID, CLIENT_ID, CLIENT_SECRET }) {
  const response = await fetch(`https://login.microsoftonline.com/${TENANT_ID}/oauth2/token`, {
    method: 'POST',
    body: new URLSearchParams({
      grant_type: 'client_credentials',
      client_id: CLIENT_ID,
      client_secret: CLIENT_SECRET,
      resource: 'https://manage.devcenter.microsoft.com',
    }),
  })
  if (!response.ok) throw new Error(`no token: HTTP ${response.status}`)
  return (await response.json()).access_token
}

function client(bearer, product) {
  return async (method, path = '', body) => {
    const response = await fetch(`${API}/${product}${path}`, {
      method,
      headers: { Authorization: `Bearer ${bearer}`, 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
    const text = await response.text()
    if (!response.ok) throw new Error(`${method} ${path || '/'}: HTTP ${response.status} ${text.slice(0, 500)}`)
    return text ? JSON.parse(text) : null
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function main() {
  const [tag, flag] = process.argv.slice(2)
  if (!/^v\d+\.\d+\.\d+$/.test(tag ?? '') || (flag && flag !== '--dry-run')) {
    console.error('usage: node packaging/store/submit-listing.mjs <vX.Y.Z> [--dry-run]')
    process.exit(2)
  }
  const dryRun = flag === '--dry-run'
  const env = process.env
  for (const name of ['TENANT_ID', 'CLIENT_ID', 'CLIENT_SECRET', 'PRODUCT_ID']) {
    if (!env[name]) throw new Error(`${name} is not set`)
  }

  const listings = listingsAsReleased(tag)
  const screenshots = readScreenshots()
  const api = client(await token(env), env.PRODUCT_ID)

  const app = await api('GET')
  const pending = app[keyOf(app, 'pendingApplicationSubmission')]
  if (pending) {
    throw new Error(
      `submission ${pending.id} is already pending: finish or delete it in Partner Center, then run this again`,
    )
  }

  const created = await api('POST', '/submissions')
  const id = created.id
  console.log(`submission ${id} created from the last published one`)
  let committed = false
  try {
    const texts = applyListings(created, listings)
    for (const line of texts.skipped) console.log(`listing skipped: ${line}`)
    const images = applyScreenshots(texts.submission, screenshots)
    for (const locale of images.skipped) console.log(`screenshots skipped: ${locale} is not a language of the listing`)
    await api('PUT', `/submissions/${id}`, images.submission)
    console.log(`listings written; ${images.entries.length} screenshots pending upload`)

    if (images.entries.length > 0) {
      const zip = storedZip(images.entries.map(({ name, path }) => ({ name, data: readFileSync(path) })))
      const upload = await fetch(created.fileUploadUrl, {
        method: 'PUT',
        headers: { 'x-ms-blob-type': 'BlockBlob', 'Content-Type': 'application/zip' },
        body: zip,
      })
      if (!upload.ok) throw new Error(`the screenshots' upload: HTTP ${upload.status} ${(await upload.text()).slice(0, 300)}`)
      console.log(`screenshots uploaded, ${zip.length} bytes`)
    }

    if (dryRun) {
      const read = await api('GET', `/submissions/${id}`)
      const byLanguage = read[keyOf(read, 'listings')] ?? {}
      for (const [locale, listing] of Object.entries(byLanguage)) {
        const base = listing[keyOf(listing, 'baseListing')] ?? {}
        const statuses = (base[keyOf(base, 'images')] ?? []).map((image) => image.fileStatus)
        console.log(`${locale}: images ${statuses.join(', ') || 'none'}`)
      }
      console.log('dry run: not committed')
      return
    }

    await api('POST', `/submissions/${id}/commit`)
    committed = true
    console.log('committed; waiting for Partner Center to take it')
    for (let i = 0; i < 40; i++) {
      await sleep(30_000)
      const status = await api('GET', `/submissions/${id}/status`)
      console.log(`status: ${status.status}`)
      if (status.status === 'CommitFailed') {
        throw new Error(`commit failed: ${JSON.stringify(status.statusDetails ?? {}).slice(0, 1500)}`)
      }
      if (status.status !== 'CommitStarted') return
    }
    throw new Error('still committing after 20 minutes: see Partner Center')
  } finally {
    if (!committed) {
      await api('DELETE', `/submissions/${id}`).then(
        () => console.log(`submission ${id} deleted`),
        (error) => console.error(`submission ${id} could not be deleted: ${error.message}`),
      )
    }
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message)
    process.exit(1)
  })
}
