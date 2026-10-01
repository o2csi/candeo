import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { test } from 'node:test'
import { deflateRawSync } from 'node:zlib'

import { entry, signatureSha256 } from './signature.mjs'

/**
 * A ZIP of the given files, deflated, as a package is. With `zip64`, it is
 * written as the Store writes it: the 32-bit fields at their maximum, and the
 * real values in ZIP64 records.
 */
function zip(files, { zip64 = false } = {}) {
  const locals = []
  const centrals = []
  let offset = 0
  for (const [name, text] of Object.entries(files)) {
    const data = Buffer.from(text)
    const packed = deflateRawSync(data)
    const nameBytes = Buffer.from(name)
    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(8, 8)
    local.writeUInt32LE(packed.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(nameBytes.length, 26)
    locals.push(local, nameBytes, packed)

    const extra = Buffer.alloc(zip64 ? 28 : 0)
    if (zip64) {
      extra.writeUInt16LE(1, 0)
      extra.writeUInt16LE(24, 2)
      extra.writeBigUInt64LE(BigInt(data.length), 4)
      extra.writeBigUInt64LE(BigInt(packed.length), 12)
      extra.writeBigUInt64LE(BigInt(offset), 20)
    }
    const central = Buffer.alloc(46)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE(8, 10)
    central.writeUInt32LE(zip64 ? 0xffffffff : packed.length, 20)
    central.writeUInt32LE(zip64 ? 0xffffffff : data.length, 24)
    central.writeUInt16LE(nameBytes.length, 28)
    central.writeUInt16LE(extra.length, 30)
    central.writeUInt32LE(zip64 ? 0xffffffff : offset, 42)
    centrals.push(central, nameBytes, extra)
    offset += 30 + nameBytes.length + packed.length
  }
  const directory = Buffer.concat(centrals)
  const count = Object.keys(files).length
  const tail = []
  if (zip64) {
    const record = Buffer.alloc(56)
    record.writeUInt32LE(0x06064b50, 0)
    record.writeBigUInt64LE(BigInt(count), 32)
    record.writeBigUInt64LE(BigInt(directory.length), 40)
    record.writeBigUInt64LE(BigInt(offset), 48)
    const locator = Buffer.alloc(20)
    locator.writeUInt32LE(0x07064b50, 0)
    locator.writeBigUInt64LE(BigInt(offset + directory.length), 8)
    tail.push(record, locator)
  }
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(zip64 ? 0xffff : count, 10)
  end.writeUInt32LE(directory.length, 12)
  end.writeUInt32LE(zip64 ? 0xffffffff : offset, 16)
  return Buffer.concat([...locals, directory, ...tail, end])
}

const files = { 'AppxManifest.xml': '<Package/>', 'AppxSignature.p7x': 'PKCX signature bytes' }
const expected = createHash('sha256').update('PKCX signature bytes').digest('hex').toUpperCase()

test('the signature hash is the hash of AppxSignature.p7x', () => {
  assert.equal(signatureSha256(zip(files)), expected)
})

test('a package written as ZIP64, as the Store writes it, reads the same', () => {
  assert.equal(signatureSha256(zip(files, { zip64: true })), expected)
  assert.equal(entry(zip(files, { zip64: true }), 'AppxManifest.xml').toString(), '<Package/>')
})

test('an unsigned package, or no ZIP at all, says so', () => {
  assert.throws(() => signatureSha256(zip({ 'AppxManifest.xml': '<Package/>' })), /not signed/)
  assert.throws(() => signatureSha256(Buffer.from('not a zip')), /not a ZIP/)
})
