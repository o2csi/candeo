// Carries the Store listings kept in this folder into a Partner Center
// submission (#162), so what the Store shows is what was reviewed here. The
// *What's new* is not in the listings: each change users will see adds its line
// to `news/`, and a release gathers the lines added since the previous one (#220).
//
//   node packaging/store/listing.mjs <version>                   what would be sent
//   node packaging/store/listing.mjs <version> <submission.json> the submission, updated
//   node packaging/store/listing.mjs --preview                   the release pull request's comment
//
// The submission is what `msstore submission get` prints. The updated one goes to
// standard output, for `msstore submission update`. Exit code 3 means no line was
// added since the previous release: Store users would be told nothing, and the
// submission must stay a draft until someone writes its What's new.
import { execFileSync } from 'node:child_process'
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const NO_NEWS = 3

const HERE = dirname(fileURLToPath(import.meta.url))
const NEWS = 'news'
// Marks the release pull request's comment, so each merge edits it rather than adding one.
export const PREVIEW_MARKER = '<!-- store-news -->'

// Partner Center's limits, checked here rather than discovered at certification.
const RELEASE_NOTES_MAX = 1500
const FEATURES_MAX = 20
const FEATURE_LENGTH_MAX = 200
const KEYWORDS_MAX = 7
const KEYWORD_LENGTH_MAX = 30

/**
 * One listing file: its locale and the four texts the submission carries. The
 * sections are read by position, not by title: the French file titles them in
 * French.
 */
export function parseListing(markdown) {
  const title = /^# .*\(([a-z]{2}-[A-Z]{2})\)\s*$/m.exec(markdown)
  if (!title) throw new Error('the title names no locale, as in "(en-US)"')

  const sections = markdown.split(/^## /m).slice(1)
  if (sections.length !== 4) {
    throw new Error(
      `4 sections expected (short description, description, features, search terms), found ${sections.length}`,
    )
  }
  const [shortDescription, description, features, keywords] = sections.map((section) =>
    section.slice(section.indexOf('\n') + 1).trim(),
  )

  // One feature per list item, as Partner Center shows them.
  const items = features
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^[-•]\s/.test(line))
    .map((line) => line.replace(/^[-•]\s+/, ''))
  if (items.length === 0) throw new Error('the features section lists nothing')
  if (items.length > FEATURES_MAX) throw new Error(`${items.length} features, the Store takes ${FEATURES_MAX}`)
  const longFeature = items.find((item) => item.length > FEATURE_LENGTH_MAX)
  if (longFeature) throw new Error(`feature "${longFeature}" is over ${FEATURE_LENGTH_MAX} characters`)

  const terms = keywords.split(',').map((term) => term.trim()).filter(Boolean)
  if (terms.length > KEYWORDS_MAX) throw new Error(`${terms.length} search terms, the Store takes ${KEYWORDS_MAX}`)
  const long = terms.find((term) => term.length > KEYWORD_LENGTH_MAX)
  if (long) throw new Error(`search term "${long}" is over ${KEYWORD_LENGTH_MAX} characters`)

  return {
    locale: title[1].toLowerCase(),
    fields: { shortDescription, description, features: items, keywords: terms },
  }
}

/** The language a fragment names a listing by: `en` for `en-us`. */
export function languageOf(listing) {
  return listing.locale.split('-')[0]
}

/** One fragment of `news/`: a line per language, `en: …`, nothing else. */
export function parseFragment(text, languages) {
  const lines = {}
  for (const line of text.split('\n').map((line) => line.trim()).filter(Boolean)) {
    const match = /^([a-z]{2}):\s*(.+)$/.exec(line)
    if (!match || !languages.includes(match[1])) {
      throw new Error(`"${line}" is not a line in ${languages.map((language) => `"${language}: …"`).join(' or ')}`)
    }
    if (lines[match[1]]) throw new Error(`two ${match[1]} lines`)
    lines[match[1]] = match[2]
  }
  const missing = languages.filter((language) => !lines[language])
  if (missing.length > 0) throw new Error(`no ${missing.join(', ')} line`)
  return lines
}

/**
 * Each language's *What's new*: a bullet per fragment, in the order of their
 * file names, which is how an author puts one change first.
 */
export function whatsNew(fragments, languages) {
  const parsed = Object.keys(fragments)
    .sort()
    .map((name) => {
      try {
        return parseFragment(fragments[name], languages)
      } catch (error) {
        throw new Error(`${name}: ${error.message}`)
      }
    })
  return Object.fromEntries(
    languages.map((language) => {
      const notes = parsed.map((lines) => `• ${lines[language]}`).join('\n')
      if (notes.length > RELEASE_NOTES_MAX) {
        throw new Error(`the ${language} What's new holds ${notes.length} characters, the Store takes ${RELEASE_NOTES_MAX}`)
      }
      return [language, notes]
    }),
  )
}

/** The release pull request's comment: the *What's new* as the release will send it. */
export function previewComment(since, news, problem) {
  const lines = [PREVIEW_MARKER, "### Microsoft Store: What's new", '']
  if (problem) {
    lines.push(`The lines added to \`packaging/store/news/\` since ${since} cannot be sent: ${problem}.`)
  } else if (!news) {
    lines.push(
      `No line added to \`packaging/store/news/\` since ${since}. Without one, the Store submission waits as a draft.`,
    )
  } else {
    lines.push(`Gathered from \`packaging/store/news/\` since ${since}, sent with this release.`)
    for (const [language, notes] of Object.entries(news)) lines.push('', `**${language}**`, '', notes)
  }
  return lines.join('\n')
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
export function readListings(folder = HERE) {
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

function isFragment(path) {
  return /\.md$/.test(path) && !/(^|\/)README\.md$/.test(path)
}

/** Every fragment of `news/`, by path. */
export function readNews(folder = HERE) {
  return Object.fromEntries(
    readdirSync(join(folder, NEWS))
      .map((name) => `${NEWS}/${name}`)
      .filter(isFragment)
      .map((path) => [path, readFileSync(join(folder, path), 'utf8')]),
  )
}

function git(cwd, ...args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8' }).trim()
}

/**
 * The release the *What's new* counts from: the last tag before `ref`, leaving
 * out `version`'s own tag, which the commit being released already carries.
 */
export function previousRelease(version, ref = 'HEAD', cwd = HERE) {
  const exclude = version ? ['--exclude', `v${version}`] : []
  return git(cwd, 'describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*', ...exclude, ref)
}

/**
 * The fragments added between `since` and `ref`, by path from `cwd`. Added only:
 * a fragment released and edited later stays with its release.
 */
export function newsAdded(since, ref = 'HEAD', cwd = HERE) {
  return git(cwd, 'diff', '--relative', '--name-only', '--no-renames', '--diff-filter=A', since, ref, '--', NEWS)
    .split('\n')
    .filter(isFragment)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, submissionPath] = process.argv.slice(2)
  const preview = version === '--preview'
  if (!preview && !/^\d+\.\d+\.\d+$/.test(version ?? '')) {
    console.error('usage: node packaging/store/listing.mjs <version> [submission.json] | --preview')
    process.exit(2)
  }

  const listings = readListings()
  const languages = listings.map(languageOf)
  const since = previousRelease(preview ? undefined : version)
  const added = newsAdded(since)
  const fragments = Object.fromEntries(added.map((path) => [path, readFileSync(join(HERE, path), 'utf8')]))

  if (preview) {
    let comment
    try {
      comment = previewComment(since, added.length > 0 ? whatsNew(fragments, languages) : null)
    } catch (error) {
      comment = previewComment(since, null, error.message)
    }
    console.log(comment)
    process.exit(0)
  }

  if (added.length === 0) {
    console.error(`No line added to packaging/store/news/ since ${since}.`)
    process.exit(NO_NEWS)
  }
  const news = whatsNew(fragments, languages)
  const complete = listings.map((listing) => ({
    ...listing,
    fields: { ...listing.fields, releaseNotes: news[languageOf(listing)] },
  }))

  if (!submissionPath) {
    console.log(JSON.stringify(complete, null, 2))
    process.exit(0)
  }
  // `msstore` writes JSON; PowerShell redirection may have put a BOM before it.
  const submission = JSON.parse(readFileSync(submissionPath, 'utf8').replace(/^﻿/, ''))
  const { submission: updated, skipped } = applyListings(submission, complete)
  for (const line of skipped) console.error(`skipped ${line}`)
  console.log(JSON.stringify(updated))
}
