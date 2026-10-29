import type { Metric, Period, Widget, WidgetStyle } from '~/types/engine'
import { NO_SERVER } from './integrations'
// Relative, not `~`: the unit tests import this file without Nuxt's alias.
import { t } from './i18n'

export type WidgetKind = Widget['kind']

const KINDS: WidgetKind[] = [
  'digital_clock', 'analog_clock', 'system_monitor', 'network', 'countdown', 'world_clocks', 'calendar',
  'timer', 'battery', 'now_playing', 'temperatures', 'top_processes', 'latency', 'volume', 'moon',
  'progress', 'stopwatch', 'counter', 'word_clock', 'phrases',
]

/** Widgets of Home Assistant, listed apart. */
const HASS_KINDS: WidgetKind[] = ['hass_tile', 'hass_entity', 'hass_graph', 'hass_entities', 'hass_picture', 'hass_weather', 'hass_template']

/** `now_playing` is found in the messages as `nowPlaying`. */
function messageKey(id: string) {
  return id.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
}

export const WIDGET_KINDS: { kind: WidgetKind, readonly label: string }[] = KINDS
  .map(kind => ({ kind, get label() { return t(`widgets.kinds.${messageKey(kind)}`) } }))

export const HASS_WIDGET_KINDS: { kind: WidgetKind, readonly label: string }[] = HASS_KINDS
  .map(kind => ({ kind, get label() { return t(`widgets.kinds.${messageKey(kind)}`) } }))

/** The most each widget of several entities shows. */
export const HASS_ENTITIES_MAX = 3

export const PERIODS: { period: Period, readonly label: string }[] = (['day', 'week', 'month', 'year'] as const)
  .map(period => ({ period, get label() { return t(`widgets.periods.${period}`) } }))

/** The world clocks widget shows up to this many places. */
export const WORLD_CLOCKS_MAX = 3

export const METRICS: { metric: Metric, readonly label: string }[] = (['cpu', 'ram', 'gpu', 'disk'] as const)
  .map(metric => ({ metric, get label() { return t(`widgets.metrics.${metric}`) } }))

const DEFAULT_STYLE: WidgetStyle = {
  color: { r: 255, g: 255, b: 255 },
  accent: { r: 56, g: 189, b: 248 },
  font_family: 'Inter',
}

/** `date` as the `YYYY-MM-DDTHH:MM` of a datetime-local field, in local time. */
export function localDateTime(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

/** A widget of `kind` with its usual options, keeping the look of the one it replaces. */
export function newWidget(kind: WidgetKind, previous: Widget | null, now = new Date()): Widget {
  // A JSON copy: `previous` comes from the reactive draft, whose proxies structuredClone rejects.
  const style = JSON.parse(JSON.stringify(previous?.style ?? DEFAULT_STYLE)) as WidgetStyle
  switch (kind) {
    case 'digital_clock':
      return { kind, seconds: false, hour12: false, second_line: 'weekday_date', zone: null, label: '', style }
    case 'analog_clock':
      return { kind, seconds: true, date: true, zone: null, label: '', style }
    case 'system_monitor':
      return { kind, metrics: ['cpu', 'ram', 'gpu'], graph: false, minutes: 5, style }
    case 'network':
      return { kind, graph: false, minutes: 5, style }
    case 'countdown': {
      const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 9, 0)
      return { kind, target: localDateTime(tomorrow), label: '', done: '', style }
    }
    case 'world_clocks':
      return { kind, clocks: [{ zone: 'America/New_York', label: '' }, { zone: 'Asia/Tokyo', label: '' }], hour12: false, style }
    case 'timer':
      return { kind, seconds: 25 * 60, label: '', done: '', style }
    case 'top_processes':
      return { kind, by: 'cpu', style }
    case 'latency':
      return { kind, host: '', style }
    case 'progress':
      return { kind, periods: ['day', 'week', 'month', 'year'], style }
    case 'stopwatch':
      return { kind, label: '', style }
    case 'counter':
      return { kind, label: '', step: 1, style }
    case 'phrases':
      return { kind, phrases: [t('widgets.defaultPhrases.first'), t('widgets.defaultPhrases.second')], seconds: 10, style }
    case 'hass_entity':
      return { kind, server: NO_SERVER, entity: '', attribute: null, label: '', icon: null, icons: {}, style }
    case 'hass_graph':
      return { kind, server: NO_SERVER, entity: '', hours: 24, label: '', style }
    case 'hass_entities':
      return { kind, server: NO_SERVER, entities: [{ entity: '', label: '' }, { entity: '', label: '' }], style }
    case 'hass_picture':
      return { kind, server: NO_SERVER, entity: '', seconds: 10, style }
    case 'hass_weather':
      return { kind, server: NO_SERVER, entity: '', forecast: true, style }
    case 'hass_template':
      return { kind, server: NO_SERVER, template: '', label: '', style }
    case 'hass_tile':
      return { kind, server: NO_SERVER, entity: '', label: '', drawing: 'auto', extra: null, min: null, max: null, hours: 24, icon: null, icons: {}, style }
    case 'calendar':
    case 'battery':
    case 'now_playing':
    case 'temperatures':
    case 'volume':
    case 'moon':
    case 'word_clock':
      return { kind, style }
  }
}

/** The city of an IANA time zone, as people read it: `America/New_York` is New York. */
export function zoneCity(zone: string): string {
  return (zone.split('/').pop() ?? zone).replaceAll('_', ' ')
}

/** Whether a look shows live content, which the UI must reload now and then. */
export function hasWidget(appearance: { layers: readonly { type: string }[] }): boolean {
  return appearance.layers.some(layer => layer.type === 'widget')
}
