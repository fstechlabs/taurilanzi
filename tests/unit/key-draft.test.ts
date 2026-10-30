import { expect, mock, test } from 'bun:test'
import { toRaw } from 'vue'
import type { Key, LayerConfig, Widget } from '../../src-nuxt/app/ipc/bindings'
import * as color from '../../src-nuxt/app/lib/color'
import * as hass from '../../src-nuxt/app/lib/actions/hass'
import * as http from '../../src-nuxt/app/lib/actions/http'
import * as obs from '../../src-nuxt/app/lib/actions/obs'

// useKeyEditor relies on Nuxt's `~` alias and auto-imports, which bun does not provide.
Object.assign(globalThis, { toRaw })
mock.module('~/ipc/bindings', () => ({ commands: {} }))
mock.module('~/composables/usePages', () => ({ isNavigation: () => false }))
mock.module('~/lib/color', () => color)
mock.module('~/lib/iconify', () => ({ iconSvg: async () => '' }))
mock.module('~/lib/actions/hass', () => hass)
mock.module('~/lib/integrations', () => ({ NO_SERVER: '00000000-0000-0000-0000-000000000000' }))
mock.module('~/lib/actions/http', () => http)
mock.module('~/lib/actions/obs', () => obs)

const { appearanceFrom, draftFromKey, keyFromDraft } = await import('../../src-nuxt/app/composables/useKeyEditor')

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
