import { expect, test } from 'bun:test'
import { moveItem } from '../../src-nuxt/app/composables/useSortable'

test('moveItem moves an item forward and back', () => {
  const list = ['a', 'b', 'c', 'd']
  moveItem(list, 0, 2)
  expect(list).toEqual(['b', 'c', 'a', 'd'])
  moveItem(list, 3, 0)
  expect(list).toEqual(['d', 'b', 'c', 'a'])
})

test('moveItem to the same place or the end', () => {
  const list = [1, 2, 3]
  moveItem(list, 1, 1)
  expect(list).toEqual([1, 2, 3])
  moveItem(list, 0, 2)
  expect(list).toEqual([2, 3, 1])
})
