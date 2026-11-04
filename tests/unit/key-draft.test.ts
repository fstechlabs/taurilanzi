import { describe, expect, mock, test } from 'bun:test'
import { toRaw } from 'vue'
import type { Action, Key, LayerConfig, Widget } from '../../src-nuxt/app/ipc/bindings'
import * as color from '../../src-nuxt/app/lib/color'
import * as hass from '../../src-nuxt/app/lib/actions/hass'
import * as http from '../../src-nuxt/app/lib/actions/http'
import * as obs from '../../src-nuxt/app/lib/actions/obs'

// useKeyEditor relies on Nuxt's `~` alias and auto-imports, which bun does not provide.
Object.assign(globalThis, { toRaw })
mock.module('~/ipc/bindings', () => ({ commands: {} }))
const NAVIGATION = ['open_page', 'back', 'home', 'previous_page', 'next_page', 'switch_profile']
mock.module('~/composables/usePages', () => ({ isNavigation: (action: Action) => NAVIGATION.includes(action.type) }))
mock.module('~/lib/color', () => color)
mock.module('~/lib/iconify', () => ({ iconSvg: async () => '' }))
mock.module('~/lib/actions/hass', () => hass)
mock.module('~/lib/integrations', () => ({ NO_SERVER: '00000000-0000-0000-0000-000000000000' }))
mock.module('~/lib/actions/http', () => http)
mock.module('~/lib/actions/obs', () => obs)

const { appearanceFrom, canonical, draftFromKey, keyDraftError, keyFromDraft, kindOf, normalized } = await import('../../src-nuxt/app/composables/useKeyEditor')

const widget: Widget = {
  kind: 'digital_clock',
  seconds: true,
  hour12: false,
  second_line: 'date',
  zone: 'Asia/Tokyo',
  label: 'Tokio',
  style: { color: { r: 255, g: 255, b: 255 }, accent: { r: 56, g: 189, b: 248 }, font_family: 'Inter' },
}

const layers: LayerConfig[] = [
  { type: 'image', asset: 'a1', fit: 'cover', inset: 0, tint: null, icon: null, source: null, crop: null },
  { type: 'widget', widget },
  { type: 'text', content: 'Hora', font_family: 'Inter', font_weight: 600, size: 18, color: { r: 250, g: 250, b: 250 }, position: 'bottom', outline: null },
  { type: 'border', color: { r: 0, g: 0, b: 0 }, width: 2, radius: 12 },
]

const key: Key = {
  kind: 'two_states',
  appearance: { background: { type: 'solid', color: { r: 17, g: 24, b: 39 } }, layers },
  actions: [],
  double_press: [],
  long_press: [],
  second_state: {
    appearance: { background: { type: 'linear_gradient', from: { r: 0, g: 0, b: 0 }, to: { r: 255, g: 0, b: 102 }, angle: 30 }, layers: [{ type: 'widget', widget }] },
    actions: [{ type: 'obs', command: { kind: 'record', mode: 'toggle' } }],
  },
}

test('a widget goes between the image and the text and comes back the same', () => {
  const draft = draftFromKey(key)
  expect(draft.widget).toEqual(widget)
  expect(draft.widget).not.toBe(widget)
  expect(draft.untouched).toEqual([layers[3]!])
  expect(appearanceFrom(draft)).toEqual(key.appearance)
  expect(keyFromDraft(draft)).toEqual(key)
})

test('the widget is placed between the image and the text whatever order it came in', () => {
  const shuffled = [layers[2]!, layers[1]!, layers[0]!]
  const draft = draftFromKey({ ...key, appearance: { ...key.appearance, layers: shuffled } })
  expect(appearanceFrom(draft).layers.map(layer => layer.type)).toEqual(['image', 'widget', 'text'])
})

test('a second state keeps its own widget', () => {
  const draft = draftFromKey(key)
  expect(draft.second!.widget).toEqual(widget)
  expect(draft.second!.widget).not.toBe(draft.widget)
  expect(appearanceFrom(draft.second!)).toEqual(key.second_state!.appearance)
})

test('several widgets come back in the order a press goes through them', () => {
  const calendar: Widget = { kind: 'calendar', style: widget.style }
  const moon: Widget = { kind: 'moon', style: widget.style }
  const appearance = {
    ...key.appearance,
    layers: [layers[1]!, { type: 'widget', widget: calendar }, layers[2]!, { type: 'widget', widget: moon }] as LayerConfig[],
  }
  const draft = draftFromKey({ ...key, appearance })
  expect(draft.widget).toEqual(widget)
  expect(draft.moreWidgets).toEqual([calendar, moon])
  expect(appearanceFrom(draft).layers.map(layer => layer.type === 'widget' ? layer.widget.kind : layer.type))
    .toEqual(['digital_clock', 'calendar', 'moon', 'text'])
})

test('a preview holds only the widget being edited', () => {
  const calendar: Widget = { kind: 'calendar', style: widget.style }
  const draft = draftFromKey(key)
  draft.moreWidgets.push(calendar)
  const kinds = (shown?: number) => appearanceFrom(draft, shown).layers
    .flatMap(layer => layer.type === 'widget' ? [layer.widget.kind] : [])
  expect(kinds()).toEqual(['digital_clock', 'calendar'])
  expect(kinds(0)).toEqual(['digital_clock'])
  expect(kinds(1)).toEqual(['calendar'])
  expect(kinds(5)).toEqual(['digital_clock', 'calendar'])
})

const look = { background: { type: 'solid', color: { r: 0, g: 0, b: 0 } }, layers: [] } as Key['appearance']
const url: Action = { type: 'open_url', url: 'https://example.com' }
const record: Action = { type: 'obs', command: { kind: 'record', mode: 'toggle' } }
const plugin: Action = { type: 'plugin', action: 'com.example.plugin.action', id: 'p1', settings: {} }

function plain(changes: Partial<Key> = {}): Key {
  return { appearance: look, actions: [], double_press: [], long_press: [], second_state: null, ...changes }
}

describe('a key saved before keys had a kind', () => {
  test('without actions is basic', () => {
    expect(kindOf(plain()).kind).toBe('basic')
  })

  test('with actions is its first one', () => {
    const draft = draftFromKey(plain({ actions: [url, { type: 'home' }] }))
    expect(draft.kind).toBe('action')
    expect(draft.main).toEqual(url)
    expect(draft.actions).toEqual([{ type: 'home' }])
  })

  test('is the action of its plugin, which goes first', () => {
    const draft = draftFromKey(plain({ actions: [url, plugin] }))
    expect(draft.kind).toBe('action')
    expect(draft.main).toEqual(plugin)
    expect(draft.actions).toEqual([url])
  })

  test('is the action of Home Assistant whose entity it shows', () => {
    const tile: Widget = { kind: 'hass_tile', server: 's', entity: 'light.desk', label: '', drawing: 'auto', extra: null, min: null, max: null, hours: 24, icon: null, icons: {}, style: widget.style }
    const hass: Action = { type: 'home_assistant', server: 's', command: { kind: 'switch', entity: 'light.desk', mode: 'toggle' } }
    const draft = draftFromKey(plain({ actions: [url, hass], appearance: { ...look, layers: [{ type: 'widget', widget: tile }] } }))
    expect(draft.kind).toBe('action')
    expect(draft.main).toEqual(hass)
    expect(draft.widget).toEqual(tile)
  })

  test('with live content and no action that draws it shows that content', () => {
    const draft = draftFromKey(plain({ actions: [url], appearance: { ...look, layers: [{ type: 'widget', widget }] } }))
    expect(draft.kind).toBe('widget')
    expect(draft.main).toBeNull()
    expect(draft.actions).toEqual([url])
  })

  test('with several live contents is a carousel', () => {
    const calendar: Widget = { kind: 'calendar', style: widget.style }
    const layers: LayerConfig[] = [{ type: 'widget', widget }, { type: 'widget', widget: calendar }]
    expect(kindOf(plain({ appearance: { ...look, layers } })).kind).toBe('carousel')
    expect(kindOf(plain({ kind: 'widget', appearance: { ...look, layers } })).kind).toBe('carousel')
  })

  test('showing the page number is a page indicator', () => {
    const text: LayerConfig = { type: 'text', content: '{page_number}/{page_count}', font_family: 'Inter', font_weight: 700, size: 40, color: { r: 255, g: 255, b: 255 }, position: 'middle', outline: null }
    expect(kindOf(plain({ appearance: { ...look, layers: [text] } })).kind).toBe('page_indicator')
  })

  test('of two states that follow its first command is that command', () => {
    const draft = draftFromKey(plain({ actions: [record, url], second_state: { appearance: look, actions: [record] } }))
    expect(draft.kind).toBe('action')
    expect(draft.main).toEqual(record)
    expect(draft.actions).toEqual([url])
    expect(draft.second!.main).toEqual(record)
    expect(draft.second!.actions).toEqual([])
  })

  test('of two states that do different things is of two states', () => {
    const draft = draftFromKey(plain({ actions: [url], second_state: { appearance: look, actions: [{ type: 'home' }] } }))
    expect(draft.kind).toBe('two_states')
    expect(draft.main).toBeNull()
    expect(draft.actions).toEqual([url])
    expect(draft.second!.actions).toEqual([{ type: 'home' }])
  })

  test('is not saved again just for being read', () => {
    const old = plain({ actions: [url, plugin] })
    expect(normalized(old)).toBe(canonical(keyFromDraft(draftFromKey(old))))
    expect(normalized(keyFromDraft(draftFromKey(old)))).toBe(normalized(old))
  })
})

test('a key keeps the kind it was saved with', () => {
  expect(kindOf(plain({ kind: 'basic', actions: [url] })).kind).toBe('basic')
  expect(draftFromKey(plain({ kind: 'basic', actions: [url] })).main).toBeNull()
  expect(kindOf(plain({ kind: 'widget' })).kind).toBe('widget')
})

test('a key of the action kind left without actions is basic', () => {
  expect(kindOf(plain({ kind: 'action' })).kind).toBe('basic')
})

test('what the key is runs before its other actions, in both states', () => {
  const draft = draftFromKey(plain({ kind: 'action', actions: [record], second_state: { appearance: look, actions: [record] } }))
  draft.actions.push(url)
  draft.second!.actions.push({ type: 'home' })
  const saved = keyFromDraft(draft)
  expect(saved.kind).toBe('action')
  expect(saved.actions).toEqual([record, url])
  expect(saved.second_state!.actions).toEqual([record, { type: 'home' }])
})

test('the action the key is counts when only one may move the dock', () => {
  const draft = draftFromKey(plain({ kind: 'action', actions: [{ type: 'back' }, { type: 'home' }] }))
  expect(draft.main).toEqual({ type: 'back' })
  expect(keyDraftError(draft)).toBe('Una tecla solo puede mover el dock una vez')
  draft.actions = [url]
  expect(keyDraftError(draft)).toBeNull()
})
