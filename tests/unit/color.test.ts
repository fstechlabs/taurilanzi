import { describe, expect, test } from 'bun:test'
import { hexToHsv, hsvToHex, normalizeHex } from '../../src-nuxt/app/lib/color'

describe('color conversions', () => {
  test('hex and hsv go back and forth', () => {
    for (const hex of ['#000000', '#ffffff', '#ff0066', '#3300cc', '#1f2937', '#00ff88']) {
      expect(hsvToHex(hexToHsv(hex))).toBe(hex)
    }
    expect(hexToHsv('#ff0000')).toEqual({ h: 0, s: 1, v: 1 })
  })

  test('reads what people type as a color', () => {
    expect(normalizeHex('#ABC')).toBe('#aabbcc')
    expect(normalizeHex(' 00ff88 ')).toBe('#00ff88')
    expect(normalizeHex('#12345')).toBeNull()
    expect(normalizeHex('rojo')).toBeNull()
  })
})
