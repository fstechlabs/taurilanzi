import { expect, mock, test } from 'bun:test'
import type { Action } from '../../src-nuxt/app/ipc/bindings'
import type { KeyDraft } from '../../src-nuxt/app/composables/useKeyEditor'
import type { CatalogEntry } from '../../src-nuxt/app/lib/actions/catalog'
import * as newKey from '../../src-nuxt/app/lib/new-key'
import * as widgets from '../../src-nuxt/app/lib/widgets'

const url: Action = { type: 'open_url', url: '' }
const plugin: Action = { type: 'plugin', action: 'com.example.plugin.action', id: 'p1', settings: {} }

function entry(id: string, changes: Partial<CatalogEntry> = {}): CatalogEntry {
  return { id, section: 'system', name: id, description: '', icon: {} as CatalogEntry['icon'], ...changes }
}

const BASIC = entry('basic', { section: 'basic', setup: 'basic' })
const OPEN_URL = entry('open_url', { create: () => structuredClone(url) })
const PLUGIN = entry('plugin:x', { plugin: true, create: () => structuredClone(plugin) })
const CLOCK = entry('widget:digital_clock', { section: 'live', setup: 'widget', widget: 'digital_clock' })
const CAROUSEL = entry('carousel', { section: 'live', setup: 'carousel' })
const TWO_STATES = entry('toggle_sequences', { setup: 'two_states' })
const SWITCH_HOTKEY = entry('switch_hotkey', { setup: 'switch_hotkey' })
const INDICATOR = entry('page_indicator', { setup: 'page_indicator' })
const ENTRIES = [BASIC, OPEN_URL, PLUGIN, CLOCK, CAROUSEL, TWO_STATES, SWITCH_HOTKEY, INDICATOR]

// key-kinds relies on Nuxt's `~` alias, which bun does not provide.
mock.module('~/lib/actions/catalog', () => ({
  BASIC,
  allEntries: () => ENTRIES,
  entryFor: (action: Action) => ENTRIES.find(e => e.id === (action.type === 'plugin' ? 'plugin:x' : action.type)),
}))
mock.module('~/lib/new-key', () => newKey)
mock.module('~/lib/widgets', () => widgets)

const { applyType, hasTypeSettings, typeEntry, PAGE_INDICATOR_TEXT } = await import('../../src-nuxt/app/lib/key-kinds')

const context = { otherPages: [], profiles: [] }

function draft(): KeyDraft {
  return {
    kind: 'basic',
    background: 'solid',
    from: '#111111',
    to: '#222222',
    angle: 90,
    image: null,
    text: { content: 'Nueva', fontFamily: 'Inter', fontWeight: 700, size: 20, color: '#ffffff', position: 'middle' },
    untouched: [],
    main: null,
    actions: [{ type: 'home' }],
    widget: null,
    moreWidgets: [],
    doublePress: [],
    longPress: [],
    second: null,
  }
}

test('an action becomes what the key is, keeping its look and other actions', () => {
  const key = draft()
  applyType(key, OPEN_URL, context)
  expect(key.kind).toBe('action')
  expect(key.main).toEqual(url)
  expect(key.actions).toEqual([{ type: 'home' }])
  expect(key.text?.content).toBe('Nueva')
  expect(typeEntry(key)).toBe(OPEN_URL)
  expect(hasTypeSettings(key)).toBe(true)
})

test('a plugin draws the key: the text of a new key goes', () => {
  const key = draft()
  applyType(key, PLUGIN, context)
  expect(key.main).toEqual(plugin)
  expect(key.text).toBeNull()
})

test('live content takes the place of the text of a new key', () => {
  const key = draft()
  applyType(key, CLOCK, context)
  expect(key.kind).toBe('widget')
  expect(key.widget?.kind).toBe('digital_clock')
  expect(key.text).toBeNull()
  expect(typeEntry(key)).toBe(CLOCK)
})

test('a carousel starts with two live contents', () => {
  const key = draft()
  applyType(key, CAROUSEL, context)
  expect(key.kind).toBe('carousel')
  expect([key.widget?.kind, ...key.moreWidgets.map(w => w.kind)]).toEqual(['digital_clock', 'calendar'])
  expect(key.text).toBeNull()
  expect(typeEntry(key)).toBe(CAROUSEL)
  expect(hasTypeSettings(key)).toBe(true)
  applyType(key, CLOCK, context)
  expect(key.moreWidgets).toEqual([])
})

test('changing the type drops what the one before brought', () => {
  const key = draft()
  applyType(key, CLOCK, context)
  applyType(key, OPEN_URL, context)
  expect(key.widget).toBeNull()
  applyType(key, BASIC, context)
  expect(key.kind).toBe('basic')
  expect(key.main).toBeNull()
  expect(key.actions).toEqual([{ type: 'home' }])
  expect(typeEntry(key)).toBe(BASIC)
  expect(hasTypeSettings(key)).toBe(false)
})

test('two states copy the look into the second', () => {
  const key = draft()
  applyType(key, TWO_STATES, context)
  expect(key.kind).toBe('two_states')
  expect(key.second?.text).toEqual(key.text)
  expect(key.second?.text).not.toBe(key.text)
  expect(key.second?.actions).toEqual([])
  expect(typeEntry(key)).toBe(TWO_STATES)
})

test('switching between two shortcuts gives each state one', () => {
  const key = draft()
  applyType(key, SWITCH_HOTKEY, context)
  expect(key.kind).toBe('two_states')
  expect(key.actions[0]?.type).toBe('hotkey')
  expect(key.second?.actions.map(a => a.type)).toEqual(['hotkey'])
})

test('a page indicator writes its text, and leaving it takes the text away', () => {
  const key = draft()
  applyType(key, INDICATOR, context)
  expect(key.kind).toBe('page_indicator')
  expect(key.text?.content).toBe(PAGE_INDICATOR_TEXT)
  applyType(key, BASIC, context)
  expect(key.text).toBeNull()
})
