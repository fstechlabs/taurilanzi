import { expect, test } from 'bun:test'
import type { DeviceModel } from '../../src-nuxt/app/ipc/bindings'
import { isWide, keyAspect, keySize, placement } from '../../src-nuxt/app/lib/dock'

const square = {
  index: 0,
  kind: { image: { width: 196, height: 196 } },
  cell: { row: 0, column: 0, row_span: 1, column_span: 1 },
  bounds: { x: 0, y: 0, width: 196, height: 196 },
}
const wide = {
  index: 13,
  kind: { image: { width: 452, height: 196 } },
  cell: { row: 2, column: 3, row_span: 1, column_span: 2 },
  bounds: { x: 768, y: 512, width: 452, height: 196 },
}
const d200 = { panel: { width: 1220, height: 708 }, controls: [square, wide] } as unknown as DeviceModel

test('keys take their size and shape from their dock', () => {
  expect(keySize(d200, 13)).toEqual({ width: 452, height: 196 })
  expect(keyAspect(d200, 0)).toBe(1)
  expect(isWide(d200, 13)).toBe(true)
  expect(isWide(d200, 0)).toBe(false)
  expect(keySize(null, 3)).toEqual({ width: 196, height: 196 })
})

test('a key sits where its dock has it, in proportion to the panel', () => {
  const at = placement(d200, wide as DeviceModel['controls'][number])
  expect(Number.parseFloat(at.left)).toBeCloseTo(768 / 1220 * 100)
  expect(Number.parseFloat(at.top)).toBeCloseTo(512 / 708 * 100)
  expect(Number.parseFloat(at.width)).toBeCloseTo(452 / 1220 * 100)
})
