// Carries the Store listings kept in this folder into a Partner Center
// submission (#162), so what the Store shows is what was reviewed here.
//
//   node packaging/store/listing.mjs <version>                   what would be sent
//   node packaging/store/listing.mjs <version> <submission.json> the submission, updated
//
// The submission is what `msstore submission get` prints. The updated one goes to
// standard output, for `msstore submission update`. Exit code 3 means a listing
// does not describe <version>: its *What's new* was written for another release,
// and the submission must stay a draft until someone writes this one's.
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const NOT_THIS_VERSION = 3

// Partner Center's limits, checked here rather than discovered at certification.
const RELEASE_NOTES_MAX = 1500
const KEYWORDS_MAX = 7
const KEYWORD_LENGTH_MAX = 30

/**
 * One listing file: its locale, the version it describes, and the four texts
 * the submission carries. The sections are read by position, not by title: the
 * French file titles them in French.
 */
export function parseListing(markdown) {
  const title = /^# .*\(([a-z]{2}-[A-Z]{2})\)\s*$/m.exec(markdown)
  if (!title) throw new Error('the title names no locale, as in "(en-US)"')
  const described = /^Version described: \*\*(\d+\.\d+\.\d+)\*\*/m.exec(markdown)
  if (!described) throw new Error('no "Version described: **X.Y.Z**" line')

  const sections = markdown.split(/^## /m).slice(1)
  if (sections.length !== 4) {
    throw new Error(`4 sections expected (short description, description, what's new, search terms), found ${sections.length}`)
  }
  const [shortDescription, description, releaseNotes, keywords] = sections.map((section) => {
    const newline = section.indexOf('\n')
    return { heading: section.slice(0, newline), body: section.slice(newline + 1).trim() }
  })

  // The heading carries the version too, and the two drift apart when only one
  // is updated.
  const headed = /\((\d+\.\d+\.\d+)\)/.exec(releaseNotes.heading)
  if (!headed || headed[1] !== described[1]) {
    throw new Error(`"Version described" says ${described[1]}, the what's new heading says ${headed?.[1] ?? 'nothing'}`)
  }
  if (releaseNotes.body.length > RELEASE_NOTES_MAX) {
    throw new Error(`what's new holds ${releaseNotes.body.length} characters, the Store takes ${RELEASE_NOTES_MAX}`)
  }
  const terms = keywords.body.split(',').map((term) => term.trim()).filter(Boolean)
  if (terms.length > KEYWORDS_MAX) throw new Error(`${terms.length} search terms, the Store takes ${KEYWORDS_MAX}`)
  const long = terms.find((term) => term.length > KEYWORD_LENGTH_MAX)
  if (long) throw new Error(`search term "${long}" is over ${KEYWORD_LENGTH_MAX} characters`)

  return {
    locale: title[1].toLowerCase(),
    version: described[1],
    fields: {
      shortDescription: shortDescription.body,
      description: description.body,
      releaseNotes: releaseNotes.body,
      keywords: terms,
    },
  }
}

/** The key of `object` equal to `name` ignoring case: the API answers in camelCase, the CLI may not. */
function keyOf(object, name) {
  return Object.keys(object ?? {}).find((key) => key.toLowerCase() === name.toLowerCase())
}

/**
 * The submission with each listing's texts in place. Only fields the submission
 * already has are written — a field it lacks is one this kind of product does
 * not take, and adding it would be refused. What was skipped is returned, to
 * say so in the log.
 */
export function applyListings(submission, listings) {
  const updated = structuredClone(submission)
  const skipped = []
  const listingsKey = keyOf(updated, 'listings')
  if (!listingsKey) throw new Error('the submission has no listings')
  const byLocale = updated[listingsKey]

  for (const listing of listings) {
    const localeKey = keyOf(byLocale, listing.locale)
    if (!localeKey) {
      skipped.push(`${listing.locale}: not a language of this Store listing`)
      continue
    }
    const baseKey = keyOf(byLocale[localeKey], 'baseListing')
    if (!baseKey) {
      skipped.push(`${listing.locale}: no base listing`)
      continue
    }
    const base = byLocale[localeKey][baseKey]
    for (const [field, value] of Object.entries(listing.fields)) {
      const key = keyOf(base, field)
      if (key) base[key] = value
      else skipped.push(`${listing.locale}: no ${field} field`)
    }
  }
  return { submission: updated, skipped }
}

/** Every `listing-*.md` of this folder, parsed. */
export function readListings(folder = dirname(fileURLToPath(import.meta.url))) {
  return readdirSync(folder)
    .filter((name) => /^listing-.+\.md$/.test(name))
    .sort()
    .map((name) => {
      try {
        return parseListing(readFileSync(join(folder, name), 'utf8'))
      } catch (error) {
        throw new Error(`${name}: ${error.message}`)
      }
    })
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, submissionPath] = process.argv.slice(2)
  if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
    console.error('usage: node packaging/store/listing.mjs <version> [submission.json]')
    process.exit(2)
  }

  const listings = readListings()
  const stale = listings.filter((listing) => listing.version !== version)
  if (stale.length > 0) {
    for (const listing of stale) {
      console.error(`The ${listing.locale} listing describes ${listing.version}, not ${version}.`)
    }
    process.exit(NOT_THIS_VERSION)
  }

  if (!submissionPath) {
    console.log(JSON.stringify(listings, null, 2))
    process.exit(0)
  }
  // `msstore` writes JSON; PowerShell redirection may have put a BOM before it.
  const submission = JSON.parse(readFileSync(submissionPath, 'utf8').replace(/^﻿/, ''))
  const { submission: updated, skipped } = applyListings(submission, listings)
  for (const line of skipped) console.error(`skipped ${line}`)
  console.log(JSON.stringify(updated))
}
