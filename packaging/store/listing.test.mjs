import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import {
  PREVIEW_MARKER,
  applyListings,
  languageOf,
  newsAdded,
  parseListing,
  previewComment,
  previousRelease,
  readListings,
  readNews,
  whatsNew,
} from './listing.mjs'

const LISTING = `# Store listing — English (en-US)

Notes for whoever submits.

## Short description (shown in search results)

Lighting control.

## Description

First paragraph.

Second paragraph.

## Features (shown on the Store page)

- One feature.
- Another feature.

## Search terms (7 at most, 30 characters each)

hid, rgb, keyboard lighting
`

const LANGUAGES = ['en', 'fr']

test('a listing gives its locale and four texts', () => {
  const listing = parseListing(LISTING)
  assert.equal(listing.locale, 'en-us')
  assert.equal(languageOf(listing), 'en')
  assert.deepEqual(listing.fields, {
    shortDescription: 'Lighting control.',
    description: 'First paragraph.\n\nSecond paragraph.',
    features: ['One feature.', 'Another feature.'],
    keywords: ['hid', 'rgb', 'keyboard lighting'],
  })
})

test('a section missing is refused rather than read out of place', () => {
  const withoutTerms = LISTING.slice(0, LISTING.indexOf('## Search terms'))
  assert.throws(() => parseListing(withoutTerms), /4 sections expected/)
})

test('the Store limits are checked before submitting', () => {
  assert.throws(
    () => parseListing(LISTING.replace('hid, rgb, keyboard lighting', 'a, b, c, d, e, f, g, h')),
    /8 search terms/,
  )
  assert.throws(
    () => parseListing(LISTING.replace('- Another feature.', `- ${'x'.repeat(201)}`)),
    /over 200 characters/,
  )
  assert.throws(
    () => whatsNew({ 'news/long.md': `en: ${'x'.repeat(1500)}\nfr: court` }, LANGUAGES),
    /the en What's new holds 1502 characters, the Store takes 1500/,
  )
})

test('the what’s new is a bullet per fragment, in the order of their names', () => {
  const news = whatsNew(
    {
      'news/signal-bindings.md': 'en: A setting follows a signal.\nfr: Un paramètre suit un signal.\n',
      'news/1-status-row.md': '\nfr: Status row, un nouvel effet.\nen: Status row, a new effect.\n',
    },
    LANGUAGES,
  )
  assert.deepEqual(news, {
    en: '• Status row, a new effect.\n• A setting follows a signal.',
    fr: '• Status row, un nouvel effet.\n• Un paramètre suit un signal.',
  })
})

test('a release’s what’s new starts with its version and ends with a link to its notes', () => {
  const news = whatsNew({ 'news/a.md': 'en: One.\nfr: Un.' }, LANGUAGES, '1.2.0')
  assert.equal(news.en, 'Version 1.2.0\n\n• One.\n\nAll changes: https://github.com/o2csi/candeo/releases/tag/v1.2.0')
  assert.equal(
    news.fr,
    'Version 1.2.0\n\n• Un.\n\nTous les changements : https://github.com/o2csi/candeo/releases/tag/v1.2.0',
  )
  assert.throws(() => whatsNew({ 'news/a.md': 'de: Eins.' }, ['de'], '1.2.0'), /no What's new frame for de/)
})

test('a fragment missing a language, or holding anything else, is refused by name', () => {
  assert.throws(() => whatsNew({ 'news/a.md': 'en: Only English.' }, LANGUAGES), /news\/a\.md: no fr line/)
  assert.throws(
    () => whatsNew({ 'news/b.md': 'en: One.\nfr: Un.\nde: Eins.' }, LANGUAGES),
    /news\/b\.md: "de: Eins\." is not a line in "en: …" or "fr: …"/,
  )
  assert.throws(() => whatsNew({ 'news/c.md': 'en: One.\nen: Two.\nfr: Un.' }, LANGUAGES), /two en lines/)
})

test('the release pull request’s comment shows what will be sent, or why nothing will', () => {
  const sent = previewComment('v1.1.0', { en: '• One.', fr: '• Un.' })
  assert.ok(sent.startsWith(PREVIEW_MARKER), 'the marker comes first, to find the comment again')
  assert.match(sent, /since v1\.1\.0/)
  assert.match(sent, /\*\*en\*\*\n\n• One\.\n\n\*\*fr\*\*\n\n• Un\./)
  assert.match(previewComment('v1.1.0', null), /No line added .* since v1\.1\.0\. Without one, the Store submission waits as a draft\./)
  assert.match(previewComment('v1.1.0', null, 'news/a.md: no fr line'), /cannot be sent: news\/a\.md: no fr line\./)
})

test('the texts land in the fields the submission has, whatever their case', () => {
  const submission = {
    Listings: {
      'en-us': {
        BaseListing: { Description: 'old', Features: ['old'], ReleaseNotes: 'old', Keywords: [], Title: 'Candeo' },
      },
      'de-de': { BaseListing: { Description: 'alt' } },
    },
    ApplicationPackages: [{ FileName: 'Candeo_1.2.0_x64.msix' }],
  }
  const listing = parseListing(LISTING)
  listing.fields.releaseNotes = '• One thing.'
  const { submission: updated, skipped } = applyListings(submission, [listing])
  const base = updated.Listings['en-us'].BaseListing
  assert.equal(base.Description, 'First paragraph.\n\nSecond paragraph.')
  assert.equal(base.ReleaseNotes, '• One thing.')
  assert.deepEqual(base.Keywords, ['hid', 'rgb', 'keyboard lighting'])
  assert.deepEqual(base.Features, ['One feature.', 'Another feature.'])
  assert.equal(base.Title, 'Candeo', 'a field no listing file carries is kept')
  assert.equal(updated.Listings['de-de'].BaseListing.Description, 'alt', 'another language is kept')
  assert.deepEqual(updated.ApplicationPackages, submission.ApplicationPackages, 'the package is kept')
  assert.deepEqual(skipped, ['en-us: no shortDescription field'], 'a field the submission lacks is not added')
  assert.equal(submission.Listings['en-us'].BaseListing.Description, 'old', 'the input is not modified')
})

test('a language the Store listing does not have is skipped and said', () => {
  const { skipped } = applyListings({ listings: { 'fr-fr': { baseListing: {} } } }, [parseListing(LISTING)])
  assert.deepEqual(skipped, ['en-us: not a language of this Store listing'])
})

test('the listings and the fragments in the repository parse', () => {
  const listings = readListings()
  assert.deepEqual(listings.map((listing) => listing.locale), ['en-us', 'fr-fr'])
  whatsNew(readNews(), listings.map(languageOf))
})

test('a release gathers the fragments added since the previous one', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'candeo-news-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const repo = join(root, 'repo')
  mkdirSync(join(repo, 'news'), { recursive: true })
  // The fixture repository ignores the machine's git configuration: a global
  // `commit.gpgsign` would ask for a key.
  const empty = join(root, 'gitconfig')
  writeFileSync(empty, '')
  const env = {
    ...process.env,
    GIT_CONFIG_GLOBAL: empty,
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_AUTHOR_NAME: 'Test',
    GIT_AUTHOR_EMAIL: 'test@example.com',
    GIT_COMMITTER_NAME: 'Test',
    GIT_COMMITTER_EMAIL: 'test@example.com',
  }
  const git = (...args) => execFileSync('git', args, { cwd: repo, env, encoding: 'utf8' })
  const commit = (files, tag) => {
    for (const [path, text] of Object.entries(files)) writeFileSync(join(repo, path), text)
    git('add', '--all')
    git('commit', '--quiet', '--allow-empty', '--message', 'change')
    if (tag) git('tag', tag)
  }

  git('init', '--quiet', '--initial-branch=main')
  commit({ 'news/old.md': 'en: Old.\nfr: Ancien.' }, 'v1.0.0')
  commit({ 'news/new.md': 'en: New.\nfr: Nouveau.', 'news/old.md': 'en: Old, edited.\nfr: Ancien.' })
  commit({}, 'v1.1.0')
  commit({ 'news/next.md': 'en: Next.\nfr: Suivant.', 'news/README.md': 'How to write one.' })

  assert.equal(previousRelease('1.1.0', 'v1.1.0', repo), 'v1.0.0', "the release's own tag is left out")
  assert.deepEqual(newsAdded('v1.0.0', 'v1.1.0', repo), ['news/new.md'], 'an edited fragment stays with its release')
  assert.equal(previousRelease(undefined, 'HEAD', repo), 'v1.1.0')
  assert.deepEqual(newsAdded('v1.1.0', 'HEAD', repo), ['news/next.md'], 'the README is no fragment')
})
