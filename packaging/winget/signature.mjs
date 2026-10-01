// The hash winget checks a package's signature against (#305): the SHA-256 of
// the `AppxSignature.p7x` inside it, what `winget hash --msix` prints as
// `SignatureSha256`. Read here so the manifest can be rendered on any runner,
// winget being a Windows program.
//
// An MSIX is a ZIP, written as ZIP64 by the Store: its end record defers to a
// ZIP64 one, and an entry's sizes and offset may sit in its ZIP64 extra field.
import { createHash } from 'node:crypto'
import { inflateRawSync } from 'node:zlib'

const MAX32 = 0xffffffff
const END = 0x06054b50
const LOCATOR = 0x07064b50
const CENTRAL = 0x02014b50
const LOCAL = 0x04034b50

/** The bytes of the entry named `name` in a ZIP, or `null` when there is none. */
export function entry(zip, name) {
  const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]))
  if (end < 0 || zip.readUInt32LE(end) !== END) {
    throw new Error('not a ZIP: no end record')
  }
  let count = zip.readUInt16LE(end + 10)
  let at = zip.readUInt32LE(end + 16)
  if (at === MAX32 || count === 0xffff) {
    const locator = end - 20
    if (locator < 0 || zip.readUInt32LE(locator) !== LOCATOR) {
      throw new Error('a ZIP64 end record is announced, and missing')
    }
    const record = Number(zip.readBigUInt64LE(locator + 8))
    count = Number(zip.readBigUInt64LE(record + 32))
    at = Number(zip.readBigUInt64LE(record + 48))
  }
  for (let i = 0; i < count; i++) {
    if (zip.readUInt32LE(at) !== CENTRAL) {
      throw new Error('the central directory is damaged')
    }
    const nameLength = zip.readUInt16LE(at + 28)
    const extraLength = zip.readUInt16LE(at + 30)
    const commentLength = zip.readUInt16LE(at + 32)
    if (zip.toString('utf8', at + 46, at + 46 + nameLength) === name) {
      const method = zip.readUInt16LE(at + 10)
      let compressed = zip.readUInt32LE(at + 20)
      let size = zip.readUInt32LE(at + 24)
      let local = zip.readUInt32LE(at + 42)
      // The ZIP64 extra field holds, in this order, the fields that overflowed.
      for (let e = at + 46 + nameLength; e < at + 46 + nameLength + extraLength; ) {
        const id = zip.readUInt16LE(e)
        let q = e + 4
        if (id === 1) {
          if (size === MAX32) (size = Number(zip.readBigUInt64LE(q))), (q += 8)
          if (compressed === MAX32) (compressed = Number(zip.readBigUInt64LE(q))), (q += 8)
          if (local === MAX32) local = Number(zip.readBigUInt64LE(q))
        }
        e += 4 + zip.readUInt16LE(e + 2)
      }
      if (zip.readUInt32LE(local) !== LOCAL) {
        throw new Error(`${name}: its local header is missing`)
      }
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28)
      const data = zip.subarray(start, start + compressed)
      if (method === 0) return data
      if (method === 8) return inflateRawSync(data)
      throw new Error(`${name}: compression method ${method} is not read here`)
    }
    at += 46 + nameLength + extraLength + commentLength
  }
  return null
}

/** `SignatureSha256` of an MSIX package, in capitals as winget-pkgs writes it. */
export function signatureSha256(msix) {
  const signature = entry(msix, 'AppxSignature.p7x')
  if (!signature) {
    throw new Error('the package holds no AppxSignature.p7x: it is not signed')
  }
  return createHash('sha256').update(signature).digest('hex').toUpperCase()
}
