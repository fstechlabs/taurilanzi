import { expect, test } from 'bun:test'
import { nearestStretch, roundedPath } from '../../src-nuxt/app/lib/link-path'

test('the line runs straight between points and rounds each corner', () => {
  const d = roundedPath([{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }], 20)
  expect(d).toBe('M 0 0 L 80 0 Q 100 0 100 20 L 100 100')
})

test('corners never round more than half a short stretch', () => {
  const d = roundedPath([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 100 }], 20)
  expect(d).toContain('L 5 0 Q 10 0 10 5')
})

test('finds the stretch nearest to the pointer', () => {
  const line = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }]
  expect(nearestStretch(line, { x: 50, y: 5 })).toBe(0)
  expect(nearestStretch(line, { x: 95, y: 70 })).toBe(1)
})
