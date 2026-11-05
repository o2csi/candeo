import assert from 'node:assert/strict'
import { test } from 'node:test'

import { entry } from '../winget/signature.mjs'
import { applyScreenshots, readScreenshots, storedZip } from './submit-listing.mjs'

const SUBMISSION = {
  id: '1152921504628',
  fileUploadUrl: 'https://example.blob.core.windows.net/ingestion/x?sig=a%2Bb',
  listings: {
    'en-us': {
      baseListing: {
        description: 'Old.',
        images: [
          { fileName: 'old.png', fileStatus: 'Uploaded', id: '1', imageType: 'Screenshot' },
          { fileName: 'logo.png', fileStatus: 'Uploaded', id: '2', imageType: 'StoreLogoSquare' },
        ],
      },
    },
  },
}

test('the screenshots replace the ones the listing has, and nothing else', () => {
  const { submission, entries, skipped } = applyScreenshots(SUBMISSION, {
    'en-us': ['shots/en-us/01-effects.png', 'shots\\en-us\\02-regions.png'],
    'de-de': ['shots/de-de/01.png'],
  })
  const images = submission.listings['en-us'].baseListing.images
  assert.deepEqual(
    images.map((image) => [image.fileName, image.fileStatus, image.imageType]),
    [
      ['old.png', 'PendingDelete', 'Screenshot'],
      ['logo.png', 'Uploaded', 'StoreLogoSquare'],
      ['en-us-01-effects.png', 'PendingUpload', 'Screenshot'],
      ['en-us-02-regions.png', 'PendingUpload', 'Screenshot'],
    ],
  )
  assert.deepEqual(entries.map((e) => e.name), ['en-us-01-effects.png', 'en-us-02-regions.png'], 'flat names, in order')
  assert.deepEqual(skipped, ['de-de'], 'a language the listing does not have is said, not added')
  assert.equal(SUBMISSION.listings['en-us'].baseListing.images[0].fileStatus, 'Uploaded', 'the input is not modified')
})

test('the ZIP holds each file under its name, as a ZIP reader finds it', () => {
  const zip = storedZip([
    { name: 'en-us-01.png', data: Buffer.from('first image') },
    { name: 'fr-fr-01.png', data: Buffer.from('première image') },
  ])
  assert.equal(entry(zip, 'en-us-01.png').toString(), 'first image')
  assert.equal(entry(zip, 'fr-fr-01.png').toString(), 'première image')
  assert.equal(entry(zip, 'missing.png'), null)
})

test('the screenshots in the repository are within what the Store takes', () => {
  const byLocale = readScreenshots()
  assert.deepEqual(Object.keys(byLocale).sort(), ['en-us', 'fr-fr'])
  for (const [locale, files] of Object.entries(byLocale)) {
    assert.ok(files.length >= 1 && files.length <= 10, `${locale}: ${files.length} screenshots`)
  }
})
