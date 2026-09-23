import assert from 'node:assert/strict'
import { test } from 'node:test'
import { applyListings, parseListing, readListings } from './listing.mjs'

const LISTING = `# Store listing — English (en-US)

Notes for whoever submits.

Version described: **1.2.0**. Everything since 1.1.0.

## Short description (shown in search results)

Lighting control.

## Description

First paragraph.

Second paragraph.

## What's new in this version (1.2.0)

• One thing.
• Another.

## Search terms (7 at most, 30 characters each)

hid, rgb, keyboard lighting
`

test('a listing gives its locale, its version and four texts', () => {
  const listing = parseListing(LISTING)
  assert.equal(listing.locale, 'en-us')
  assert.equal(listing.version, '1.2.0')
  assert.deepEqual(listing.fields, {
    shortDescription: 'Lighting control.',
    description: 'First paragraph.\n\nSecond paragraph.',
    releaseNotes: '• One thing.\n• Another.',
    keywords: ['hid', 'rgb', 'keyboard lighting'],
  })
})

test('a what’s new heading that names another version is refused', () => {
  assert.throws(
    () => parseListing(LISTING.replace('this version (1.2.0)', 'this version (1.1.0)')),
    /says 1\.2\.0, the what's new heading says 1\.1\.0/,
  )
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
    () => parseListing(LISTING.replace('• Another.', 'x'.repeat(1500))),
    /the Store takes 1500/,
  )
})

test('the texts land in the fields the submission has, whatever their case', () => {
  const submission = {
    Listings: {
      'en-us': { BaseListing: { Description: 'old', ReleaseNotes: 'old', Keywords: [], Title: 'Candeo' } },
      'de-de': { BaseListing: { Description: 'alt' } },
    },
    ApplicationPackages: [{ FileName: 'Candeo_1.2.0_x64.msix' }],
  }
  const { submission: updated, skipped } = applyListings(submission, [parseListing(LISTING)])
  const base = updated.Listings['en-us'].BaseListing
  assert.equal(base.Description, 'First paragraph.\n\nSecond paragraph.')
  assert.equal(base.ReleaseNotes, '• One thing.\n• Another.')
  assert.deepEqual(base.Keywords, ['hid', 'rgb', 'keyboard lighting'])
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

test('the listings in the repository parse, and describe the same version', () => {
  const listings = readListings()
  assert.deepEqual(listings.map((listing) => listing.locale), ['en-us', 'fr-fr'])
  assert.equal(new Set(listings.map((listing) => listing.version)).size, 1)
})
