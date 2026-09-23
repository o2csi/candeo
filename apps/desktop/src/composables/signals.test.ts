import { describe, expect, it } from 'vitest'

import {
  alive,
  interfaceRows,
  signalText,
  signalsAddress,
  ticking,
  timeLeft,
  validPort,
} from './signals'

const NOW = new Date(2026, 8, 23, 14, 0, 0).getTime()

describe('signalText', () => {
  it('reads a value the way a rule compares it', () => {
    expect(signalText('failed')).toBe('failed')
    expect(signalText(1)).toBe('1')
    expect(signalText(2.0)).toBe('2')
    expect(signalText(0.4)).toBe('0.4')
    expect(signalText(true)).toBe('true')
  })
})

describe('validPort', () => {
  it('takes the ports a desktop application can listen on', () => {
    expect(validPort(7317)).toBe(true)
    expect(validPort(1024)).toBe(true)
    expect(validPort(65535)).toBe(true)
  })

  it('refuses privileged ports, ports past the range and what is not a whole number', () => {
    expect(validPort(80)).toBe(false)
    expect(validPort(1023)).toBe(false)
    expect(validPort(65536)).toBe(false)
    expect(validPort(7317.5)).toBe(false)
    expect(validPort(Number(''))).toBe(false)
    expect(validPort(Number.NaN)).toBe(false)
  })
})

describe('signalsAddress', () => {
  it('is where a sender on this computer posts', () => {
    expect(signalsAddress(7317)).toBe('http://127.0.0.1:7317/signals')
  })
})

describe('interfaceRows', () => {
  const up = [
    { name: 'Wi-Fi', addresses: ['192.0.2.23', '2001:db8::23'] },
    { name: 'Ethernet', addresses: ['198.51.100.7'] },
  ]

  it('lists the interfaces up with their first address, ticked or not', () => {
    expect(interfaceRows(up, ['Ethernet'])).toEqual([
      { name: 'Wi-Fi', address: '192.0.2.23', ticked: false },
      { name: 'Ethernet', address: '198.51.100.7', ticked: true },
    ])
  })

  it('keeps an interface ticked while it is down, so that it can be unticked', () => {
    expect(interfaceRows(up, ['Dock'])).toEqual([
      { name: 'Wi-Fi', address: '192.0.2.23', ticked: false },
      { name: 'Ethernet', address: '198.51.100.7', ticked: false },
      { name: 'Dock', address: null, ticked: true },
    ])
  })

  it('lists nothing when no interface is up and none is ticked', () => {
    expect(interfaceRows([], [])).toEqual([])
  })
})

describe('ticking', () => {
  it('adds an interface once, and removes it', () => {
    expect(ticking(['Wi-Fi'], 'Ethernet', true)).toEqual(['Wi-Fi', 'Ethernet'])
    expect(ticking(['Wi-Fi'], 'Wi-Fi', true)).toEqual(['Wi-Fi'])
    expect(ticking(['Wi-Fi', 'Ethernet'], 'Wi-Fi', false)).toEqual(['Ethernet'])
    expect(ticking([], 'Wi-Fi', false)).toEqual([])
  })
})

describe('alive', () => {
  it('drops what expired before Rust says so, and keeps what lasts until erased', () => {
    const held = [
      { name: 'build', value: 'failed', received: NOW - 1000, expires: null },
      { name: 'doorbell', value: 'ring', received: NOW - 60_000, expires: NOW },
      { name: 'volume', value: 0.4, received: NOW, expires: NOW + 60_000 },
    ]
    expect(alive(held, NOW).map((s) => s.name)).toEqual(['build', 'volume'])
  })
})

describe('timeLeft', () => {
  it('says a signal sent to last lasts until erased', () => {
    expect(timeLeft(null, NOW)).toEqual({ key: 'settings.signals.untilErased' })
  })

  it('counts the default lifetime down second by second, never under what is left', () => {
    expect(timeLeft(NOW + 60_000, NOW)).toEqual({ key: 'settings.signals.leftSeconds', n: 60 })
    expect(timeLeft(NOW + 59_200, NOW)).toEqual({ key: 'settings.signals.leftSeconds', n: 60 })
    expect(timeLeft(NOW + 119_000, NOW)).toEqual({ key: 'settings.signals.leftSeconds', n: 119 })
  })

  it('never counts below zero while Rust catches up', () => {
    expect(timeLeft(NOW - 400, NOW)).toEqual({ key: 'settings.signals.leftSeconds', n: 0 })
  })

  it('reads minutes from two minutes, rounded up', () => {
    expect(timeLeft(NOW + 120_000, NOW)).toEqual({ key: 'settings.signals.leftMinutes', n: 2 })
    expect(timeLeft(NOW + 121_000, NOW)).toEqual({ key: 'settings.signals.leftMinutes', n: 3 })
    expect(timeLeft(NOW + 7_199_000, NOW)).toEqual({ key: 'settings.signals.leftMinutes', n: 120 })
  })

  it('reads whole hours from two hours', () => {
    expect(timeLeft(NOW + 7_200_000, NOW)).toEqual({ key: 'settings.signals.leftHours', n: 2 })
    expect(timeLeft(NOW + 10_799_000, NOW)).toEqual({ key: 'settings.signals.leftHours', n: 2 })
    expect(timeLeft(NOW + 86_400_000, NOW)).toEqual({ key: 'settings.signals.leftHours', n: 24 })
  })
})
