import { expect, test } from 'bun:test'
import { i18n } from '../../src-nuxt/app/lib/i18n'

function keys(messages: unknown, prefix = ''): string[] {
  if (typeof messages !== 'object' || messages === null) {
    return [prefix]
  }
  return Object.entries(messages).flatMap(([key, value]) => keys(value, prefix ? `${prefix}.${key}` : key))
}

const messages = i18n.global.messages.value as Record<'es' | 'en', unknown>

test('Spanish and English have the same messages', () => {
  const es = keys(messages.es).sort()
  const en = keys(messages.en).sort()
  expect(es.filter(key => !en.includes(key))).toEqual([])
  expect(en.filter(key => !es.includes(key))).toEqual([])
})

test('no message is empty', () => {
  for (const locale of ['es', 'en'] as const) {
    const empty = keys(messages[locale]).filter((key) => {
      const value = key.split('.').reduce<unknown>((node, part) => (node as Record<string, unknown>)?.[part], messages[locale])
      return typeof value === 'string' && !value.trim()
    })
    expect(empty).toEqual([])
  }
})
