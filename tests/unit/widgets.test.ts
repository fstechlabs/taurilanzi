import { describe, expect, test } from 'bun:test'
import type { Widget, WidgetStyle } from '../../src-nuxt/app/ipc/bindings'
import { WIDGET_KINDS, hasWidget, localDateTime, newWidget, zoneCity } from '../../src-nuxt/app/lib/widgets'

const style: WidgetStyle = {
  color: { r: 1, g: 2, b: 3 },
  accent: { r: 4, g: 5, b: 6 },
  font_family: 'Roboto',
}

const previous: Widget = { kind: 'network', graph: true, minutes: 30, style }

describe('newWidget', () => {
  test('every kind keeps a copy of the previous look', () => {
    for (const { kind } of WIDGET_KINDS) {
      const widget = newWidget(kind, previous)
      expect(widget.kind).toBe(kind)
      expect(widget.style).toEqual(style)
      expect(widget.style).not.toBe(style)
      expect(widget.style.color).not.toBe(style.color)
    }
  })

  test('without a previous widget it takes the default look', () => {
    const widget = newWidget('digital_clock', null)
    expect(widget.style.font_family).toBe('Inter')
    expect(widget.style.color).toEqual({ r: 255, g: 255, b: 255 })
    expect(newWidget('analog_clock', null).style).not.toBe(widget.style)
  })

  test('graphs start off and five minutes long', () => {
    const monitor = newWidget('system_monitor', previous)
    const network = newWidget('network', previous)
    expect(monitor).toMatchObject({ graph: false, minutes: 5, metrics: ['cpu', 'ram', 'gpu'] })
    expect(network).toMatchObject({ graph: false, minutes: 5 })
  })

  test('clocks start in the local zone without a label', () => {
    expect(newWidget('digital_clock', null)).toMatchObject({ seconds: false, hour12: false, second_line: 'weekday_date', zone: null, label: '' })
    expect(newWidget('analog_clock', null)).toMatchObject({ seconds: true, date: true, zone: null, label: '' })
  })

  test('the new kinds start ready to show', () => {
    expect(WIDGET_KINDS).toHaveLength(20)
    expect(newWidget('world_clocks', null)).toMatchObject({ hour12: false, clocks: [{ zone: 'America/New_York', label: '' }, { zone: 'Asia/Tokyo', label: '' }] })
    expect(newWidget('timer', null)).toMatchObject({ seconds: 1500, label: '', done: '' })
    expect(newWidget('top_processes', null)).toMatchObject({ by: 'cpu' })
    expect(newWidget('latency', null)).toMatchObject({ host: '' })
    expect(newWidget('progress', null)).toMatchObject({ periods: ['day', 'week', 'month', 'year'] })
    expect(newWidget('counter', null)).toMatchObject({ label: '', step: 1 })
    expect(newWidget('phrases', null)).toMatchObject({ seconds: 10 })
    for (const kind of ['calendar', 'battery', 'now_playing', 'temperatures', 'volume', 'moon', 'word_clock'] as const) {
      expect(Object.keys(newWidget(kind, null)).sort()).toEqual(['kind', 'style'])
    }
  })

  test('a countdown points at nine tomorrow', () => {
    const widget = newWidget('countdown', null, new Date(2026, 11, 31, 22, 15))
    expect(widget).toMatchObject({ kind: 'countdown', target: '2027-01-01T09:00', label: '', done: '' })
  })
})

test('zoneCity reads the city of a time zone', () => {
  expect(zoneCity('America/New_York')).toBe('New York')
  expect(zoneCity('America/Argentina/Buenos_Aires')).toBe('Buenos Aires')
  expect(zoneCity('UTC')).toBe('UTC')
})

test('localDateTime pads every field', () => {
  expect(localDateTime(new Date(2026, 0, 5, 7, 3))).toBe('2026-01-05T07:03')
  expect(localDateTime(new Date(2026, 10, 25, 23, 59))).toBe('2026-11-25T23:59')
})

test('hasWidget looks for a widget layer', () => {
  expect(hasWidget({ layers: [] })).toBe(false)
  expect(hasWidget({ layers: [{ type: 'image' }, { type: 'text' }] })).toBe(false)
  expect(hasWidget({ layers: [{ type: 'image' }, { type: 'widget' }] })).toBe(true)
})
