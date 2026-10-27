import { expect, test } from 'bun:test'
import { linkColors } from '../../src-nuxt/app/lib/link-colors'

const hue = (color: string) => Number(color.match(/^hsl\((\d+) /)![1])

test('lines on the canvas get clearly different colours', () => {
  const ids = ['a:0', 'a:1', 'a:2', 'b:0', 'b:4', 'c:13', 'd:7']
  const hues = [...linkColors(ids).values()].map(hue)
  for (let i = 0; i < hues.length; i++) {
    for (let j = i + 1; j < hues.length; j++) {
      const d = Math.abs(hues[i]! - hues[j]!) % 360
      expect(Math.min(d, 360 - d)).toBeGreaterThanOrEqual(39)
    }
  }
})

test('a line keeps its colour when an unrelated one is added', () => {
  const before = linkColors(['a:0', 'b:1'])
  const after = linkColors(['a:0', 'b:1', 'z:9'])
  expect(after.get('a:0')).toBe(before.get('a:0'))
  expect(after.get('b:1')).toBe(before.get('b:1'))
})
