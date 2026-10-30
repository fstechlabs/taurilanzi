import type { Action, HassCommand, HassEntityInfo, HassInfo, HassLevel, HassOption, TileDrawing } from '~/types/engine'
import type { KeyDraft, StateDraft } from '~/composables/useKeyEditor'
// Relative, not `~`: the unit tests import this file and bun does not know that alias.
import { t } from '../i18n'
import { followedState as obsFollowedState } from './obs'

export type HassKind = HassCommand['kind']

export function domainOf(entity: string) {
  return entity.split('.')[0] ?? ''
}

/** Same rule as the engine: `domain.object_id` in lowercase letters, digits and underscores. */
export function isEntityId(id: string) {
  return /^[a-z0-9_]+\.[a-z0-9_]+$/.test(id)
}

/** Domains whose entities can be turned on and off in their own words; the rest go through
 * Home Assistant's generic on and off. */
const SWITCH_WORDS: Record<string, string> = {
  cover: 'openClose',
  valve: 'openClose',
  lock: 'lockUnlock',
  vacuum: 'startDock',
  lawn_mower: 'startDock',
  timer: 'startPause',
  alarm_control_panel: 'armDisarm',
  automation: 'enableDisable',
  script: 'runStop',
  update: 'installSkip',
  counter: 'upDown',
}

/** Domains with nothing to switch: they only show what they measure. */
const NOT_SWITCHABLE = new Set([
  'sensor', 'binary_sensor', 'weather', 'person', 'device_tracker', 'sun', 'zone', 'event', 'calendar', 'todo', 'image',
  'date', 'time', 'datetime', 'text', 'input_text', 'input_datetime', 'number', 'input_number', 'select', 'input_select',
  'notify', 'tts', 'stt', 'conversation', 'air_quality', 'geo_location', 'image_processing', 'wake_word',
  'assist_satellite', 'ai_task',
])

/** Domains that only run, whatever the mode. */
const PRESS_ONLY = new Set(['button', 'input_button', 'scene'])

export const RUN_DOMAINS = ['scene', 'script', 'button', 'input_button', 'automation', 'lock', 'update', 'timer', 'counter']

export const LEVELS: Record<string, HassLevel[]> = {
  light: ['brightness', 'color_temp'],
  cover: ['position', 'tilt'],
  valve: ['position'],
  fan: ['speed'],
  media_player: ['volume'],
  climate: ['temperature', 'humidity'],
  water_heater: ['temperature'],
  humidifier: ['humidity'],
  number: ['value'],
  input_number: ['value'],
  counter: ['value'],
}

interface OptionSpec {
  /** The attribute with the list; null is the fixed list of repeat modes. */
  list: string | null
  /** The attribute of the current option; null is the state. */
  current: string | null
}

/** Where each entity keeps each list of options, as the engine reads it. */
const OPTIONS: Record<string, Partial<Record<HassOption, OptionSpec>>> = {
  select: { option: { list: 'options', current: null } },
  input_select: { option: { list: 'options', current: null } },
  light: { effect: { list: 'effect_list', current: 'effect' } },
  climate: {
    hvac_mode: { list: 'hvac_modes', current: null },
    preset: { list: 'preset_modes', current: 'preset_mode' },
    fan_mode: { list: 'fan_modes', current: 'fan_mode' },
    swing_mode: { list: 'swing_modes', current: 'swing_mode' },
  },
  fan: { preset: { list: 'preset_modes', current: 'preset_mode' } },
  humidifier: { mode: { list: 'available_modes', current: 'mode' }, preset: { list: 'preset_modes', current: 'preset_mode' } },
  media_player: {
    source: { list: 'source_list', current: 'source' },
    sound_mode: { list: 'sound_mode_list', current: 'sound_mode' },
    repeat: { list: null, current: 'repeat' },
  },
  water_heater: { operation_mode: { list: 'operation_list', current: 'operation_mode' } },
  vacuum: { fan_speed: { list: 'fan_speed_list', current: 'fan_speed' } },
}

const REPEAT_MODES = ['off', 'all', 'one']

/** What each entry of the catalog acts on. */
export const DOMAINS_OF: Record<HassKind, string[] | null> = {
  switch: null,
  run: RUN_DOMAINS,
  level: Object.keys(LEVELS),
  choose: Object.keys(OPTIONS),
  media: ['media_player'],
  color: ['light'],
  alarm: ['alarm_control_panel'],
  call: null,
  assist: null,
  event: null,
  show: null,
  tile: null,
}

export function canSwitch(entity: string) {
  return !NOT_SWITCHABLE.has(domainOf(entity))
}

/** How the two ways of a switch are called for an entity, or null if it only runs. */
export function switchLabels(entity: string): { on: string, off: string } | null {
  const domain = domainOf(entity)
  if (PRESS_ONLY.has(domain)) {
    return null
  }
  const words = SWITCH_WORDS[domain] ?? 'onOff'
  return { on: t(`actions.hass.switch.${words}.on`), off: t(`actions.hass.switch.${words}.off`) }
}

/** The bit of `supported_features` each level needs, by domain, from Home Assistant's entity features. */
const LEVEL_FEATURES: Partial<Record<string, Partial<Record<HassLevel, number>>>> = {
  cover: { position: 4, tilt: 128 },
  valve: { position: 4 },
  fan: { speed: 1 },
  media_player: { volume: 4 },
  climate: { temperature: 1 | 2, humidity: 4 },
  water_heater: { temperature: 1 },
}

/** The levels of an entity's domain; with its attributes, only those the entity supports. */
export function levelsOf(entity: string, info?: HassEntityInfo): HassLevel[] {
  const domain = domainOf(entity)
  const all = LEVELS[domain] ?? []
  const attributes = info?.attributes
  if (!attributes) {
    return all
  }
  if (domain === 'light') {
    const modes = Array.isArray(attributes.supported_color_modes) ? attributes.supported_color_modes as string[] : null
    if (!modes) {
      return all
    }
    return all.filter(level => level === 'brightness' ? modes.some(mode => mode !== 'onoff') : modes.includes('color_temp'))
  }
  const features = attributes.supported_features
  if (typeof features !== 'number') {
    return all
  }
  return all.filter((level) => {
    const needs = LEVEL_FEATURES[domain]?.[level]
    return needs === undefined || (features & needs) !== 0
  })
}

export function optionsOf(entity: string): HassOption[] {
  return Object.keys(OPTIONS[domainOf(entity)] ?? {}) as HassOption[]
}

/** The options an entity offers for `option`, as Home Assistant names them. */
export function choicesOf(info: HassEntityInfo | undefined, option: HassOption): string[] {
  const spec = info ? OPTIONS[domainOf(info.id)]?.[option] : undefined
  if (!info || !spec) {
    return []
  }
  if (!spec.list) {
    return REPEAT_MODES
  }
  const list = info.attributes[spec.list]
  return Array.isArray(list) ? list.map(String) : []
}

/** The attribute that holds the current option, for its texts; null is the state. */
export function optionAttribute(entity: string, option: HassOption): string | null {
  return OPTIONS[domainOf(entity)]?.[option]?.current ?? null
}

/** The unit each level is set in. */
export function levelUnit(level: HassLevel, info?: HassEntityInfo, temperatureUnit = '°C'): string {
  switch (level) {
    case 'color_temp':
      return 'K'
    case 'temperature':
      return String(info?.attributes.temperature_unit ?? temperatureUnit)
    case 'value':
      return String(info?.attributes.unit_of_measurement ?? '')
    default:
      return '%'
  }
}

/** What is missing for a command to be saved, the same checks the engine makes. */
export function hassError(command: HassCommand): string | null {
  const entityKinds: HassKind[] = ['switch', 'run', 'level', 'choose', 'media', 'color', 'alarm', 'show', 'tile']
  if (entityKinds.includes(command.kind) && 'entity' in command && !isEntityId(command.entity)) {
    return t('actions.hass.errors.entity')
  }
  switch (command.kind) {
    case 'level': {
      const change = command.change
      if (change.type === 'step') {
        return change.by === 0 || !Number.isFinite(change.by) ? t('actions.hass.errors.step') : null
      }
      if (!Number.isFinite(change.to)) {
        return t('actions.hass.errors.value')
      }
      const range = command.level === 'color_temp'
        ? [1000, 12000]
        : ['brightness', 'position', 'tilt', 'speed', 'volume', 'humidity'].includes(command.level) ? [0, 100] : null
      return range && (change.to < range[0]! || change.to > range[1]!) ? t('actions.hass.errors.range') : null
    }
    case 'choose':
      return command.choice.type === 'set' && !command.choice.option.trim() ? t('actions.hass.errors.option') : null
    case 'call':
      if (!isEntityId(command.action.trim())) {
        return t('actions.hass.errors.action')
      }
      return command.data && typeof command.data === 'object' && !Array.isArray(command.data) ? null : t('actions.hass.errors.data')
    case 'assist':
      return command.text.trim() ? null : t('actions.hass.errors.text')
    case 'event':
      return command.event.trim() && !/\s/.test(command.event.trim()) ? null : t('actions.hass.errors.event')
    default:
      return null
  }
}

/** A state of Home Assistant a key of two states can follow, and the color of its second look. */
export interface FollowedState {
  /** Ends "muestra el estado 2 …". */
  while: string
  color: string
  /** The entity whose state it is. */
  entity: string
  /** The state of the entity the second look shows, for its icon. */
  state: string | null
}

const ON_COLOR = '#d97706'
const PLAY_COLOR = '#16a34a'
const MUTE_COLOR = '#dc2626'

/** The state in a domain's own words that counts as on, for the icon of the second look. */
const ON_STATE: Record<string, string> = {
  cover: 'open',
  valve: 'open',
  lock: 'locked',
  media_player: 'playing',
  climate: 'heat',
  vacuum: 'cleaning',
  lawn_mower: 'mowing',
  alarm_control_panel: 'armed_away',
  timer: 'active',
  person: 'home',
  device_tracker: 'home',
}

/** The state that counts as off, for the icon of the first look of a key that follows one. */
const OFF_STATE: Record<string, string> = {
  cover: 'closed',
  valve: 'closed',
  lock: 'unlocked',
  media_player: 'off',
  climate: 'off',
  vacuum: 'docked',
  lawn_mower: 'docked',
  alarm_control_panel: 'disarmed',
  timer: 'idle',
  person: 'not_home',
  device_tracker: 'not_home',
}

export function offState(entity: string) {
  return OFF_STATE[domainOf(entity)] ?? 'off'
}

/** What a key of two states with this command shows, as the engine follows it. */
export function followedState(command: HassCommand): FollowedState | null {
  const active = (entity: string): FollowedState => ({
    while: t('actions.hass.while.active'),
    color: ON_COLOR,
    entity,
    state: ON_STATE[domainOf(entity)] ?? 'on',
  })
  switch (command.kind) {
    case 'switch':
    case 'level':
    case 'color':
    case 'alarm':
      return active(command.entity)
    case 'run':
      return ['script', 'automation'].includes(domainOf(command.entity)) ? active(command.entity) : null
    case 'choose':
      return command.choice.type === 'set' && OPTIONS[domainOf(command.entity)]?.[command.option]
        ? { while: t('actions.hass.while.chosen', { option: command.choice.option }), color: ON_COLOR, entity: command.entity, state: null }
        : null
    case 'media':
      if (['play_pause', 'play', 'pause', 'stop'].includes(command.control)) {
        return { while: t('actions.hass.while.playing'), color: PLAY_COLOR, entity: command.entity, state: 'playing' }
      }
      if (command.control === 'mute') {
        return { while: t('actions.hass.while.muted'), color: MUTE_COLOR, entity: command.entity, state: null }
      }
      if (command.control === 'shuffle') {
        return { while: t('actions.hass.while.shuffling'), color: PLAY_COLOR, entity: command.entity, state: null }
      }
      return null
    case 'call':
      return command.target?.entities?.[0] ? active(command.target.entities[0]) : null
    case 'show':
    case 'tile':
      return hasOnOff(command.entity) ? active(command.entity) : null
    default:
      return null
  }
}

/** Entities that are on or off, in their own words. */
function hasOnOff(entity: string) {
  const domain = domainOf(entity)
  return Boolean(domain) && (canSwitch(entity) || ['binary_sensor', 'person', 'device_tracker', 'sun'].includes(domain)) && !PRESS_ONLY.has(domain)
}

const TILE_SWITCHES = new Set([
  'light', 'switch', 'fan', 'input_boolean', 'siren', 'humidifier', 'remote', 'group', 'automation', 'climate',
  'water_heater', 'cover', 'valve', 'lock', 'vacuum', 'lawn_mower', 'timer', 'alarm_control_panel',
])

/** What a tile of `entity` does when pressed, as the engine does it; null when it does nothing. */
export function tileAction(entity: string): string | null {
  const domain = domainOf(entity)
  if (['scene', 'script', 'button', 'input_button', 'counter'].includes(domain)) {
    return t(`actions.hass.tile.does.${domain === 'counter' ? 'increment' : 'run'}`)
  }
  if (domain === 'media_player') {
    return t('actions.hass.media.playPause')
  }
  if (domain === 'select' || domain === 'input_select') {
    return t('actions.hass.tile.does.nextOption')
  }
  const labels = TILE_SWITCHES.has(domain) ? switchLabels(entity) : null
  return labels ? t('actions.hass.tile.does.switch', labels) : null
}

const COVER_DRAWINGS: Record<string, TileDrawing> = {
  curtain: 'curtain',
  blind: 'blind',
  damper: 'blind',
  shade: 'shade',
  garage: 'garage',
  awning: 'awning',
  gate: 'gate',
  door: 'door',
  window: 'window',
}

const BINARY_DRAWINGS: Record<string, TileDrawing> = {
  door: 'door',
  opening: 'door',
  window: 'window',
  garage_door: 'garage',
  plug: 'plug',
  power: 'plug',
  light: 'bulb',
  lock: 'lock',
  moisture: 'droplet',
  battery: 'battery',
}

const GAUGE_CLASSES = new Set([
  'carbon_dioxide', 'carbon_monoxide', 'pm1', 'pm25', 'pm10', 'aqi', 'volatile_organic_compounds',
  'volatile_organic_compounds_parts', 'nitrogen_dioxide', 'ozone',
])

/** Domains whose tile has a drawing of its own: a vacuum, the sun, a timer's ring, a player's
 * artwork, a person's photo, a camera's picture, the weather. */
const OWN_DRAWING = new Set(['vacuum', 'sun', 'timer', 'media_player', 'person', 'device_tracker', 'camera', 'image', 'weather'])

function isNumeric(info?: HassEntityInfo) {
  return Boolean(info) && info!.state.trim() !== '' && Number.isFinite(Number(info!.state))
}

/** The drawing the engine picks for an entity when left to it; `auto` for those of their own. */
export function suitedDrawing(entity: string, info?: HassEntityInfo): TileDrawing {
  const domain = domainOf(entity)
  const deviceClass = String(info?.attributes.device_class ?? '')
  switch (domain) {
    case 'light':
      return 'bulb'
    case 'switch':
      return deviceClass === 'outlet' ? 'plug' : 'switch'
    case 'input_boolean':
    case 'automation':
      return 'switch'
    case 'fan':
      return 'fan'
    case 'cover':
      return COVER_DRAWINGS[deviceClass] ?? 'shutter'
    case 'valve':
      return 'valve'
    case 'lock':
      return 'lock'
    case 'climate':
    case 'water_heater':
    case 'humidifier':
    case 'number':
    case 'input_number':
      return 'gauge'
    case 'counter':
      return 'value'
    case 'sensor':
      if (!isNumeric(info)) {
        return 'icon'
      }
      if (deviceClass === 'temperature') {
        return 'thermometer'
      }
      if (deviceClass === 'humidity' || deviceClass === 'moisture') {
        return 'droplet'
      }
      if (deviceClass === 'battery') {
        return 'battery'
      }
      return GAUGE_CLASSES.has(deviceClass) || info?.attributes.unit_of_measurement === '%' ? 'gauge' : 'icon'
    case 'binary_sensor':
      return BINARY_DRAWINGS[deviceClass] ?? 'icon'
    default:
      return OWN_DRAWING.has(domain) ? 'auto' : 'icon'
  }
}

const COVERS: TileDrawing[] = ['curtain', 'blind', 'shutter', 'shade', 'garage', 'awning', 'gate', 'door', 'window']

/** The drawings a tile of `entity` can choose from, besides leaving it to the app. */
export function tileDrawings(entity: string, info?: HassEntityInfo): TileDrawing[] {
  switch (domainOf(entity)) {
    case 'light':
      return ['bulb', 'icon', 'value']
    case 'switch':
    case 'input_boolean':
      return ['plug', 'switch', 'bulb', 'fan', 'icon', 'value']
    case 'automation':
      return ['switch', 'icon']
    case 'fan':
      return ['fan', 'icon', 'value']
    case 'cover':
      return [...COVERS, 'icon', 'value']
    case 'valve':
      return ['valve', 'icon', 'value']
    case 'lock':
      return ['lock', 'icon']
    case 'climate':
    case 'water_heater':
      return ['gauge', 'thermometer', 'icon', 'value']
    case 'humidifier':
      return ['gauge', 'droplet', 'icon', 'value']
    case 'number':
    case 'input_number':
    case 'counter':
      return ['gauge', 'value', 'graph', 'icon']
    case 'sensor':
      return isNumeric(info) ? ['gauge', 'graph', 'thermometer', 'droplet', 'battery', 'value', 'icon'] : ['icon', 'value']
    case 'binary_sensor':
      return ['door', 'window', 'garage', 'plug', 'bulb', 'lock', 'droplet', 'battery', 'icon']
    default:
      return ['icon', 'value']
  }
}

/** Whether a tile drawn so takes a least and a most. */
export function drawingHasRange(drawing: TileDrawing) {
  return drawing === 'gauge' || drawing === 'thermometer' || drawing === 'droplet'
}

/** Domains whose devices often measure what they draw, which a tile shows next to them. */
const DRAWS_POWER = new Set(['switch', 'light', 'fan', 'media_player', 'climate', 'water_heater', 'humidifier', 'vacuum', 'input_boolean'])

/** A sensor of the same device as `entity` worth showing beside it: what a plug draws. */
export function suggestedExtra(entity: string, entities: readonly HassEntityInfo[]): string | null {
  const own = entities.find(e => e.id === entity)
  if (!own?.device || !DRAWS_POWER.has(domainOf(entity))) {
    return null
  }
  const siblings = entities.filter(e => e.id !== entity && e.device === own.device && domainOf(e.id) === 'sensor')
  const of = (deviceClass: string) => siblings.find(e => e.attributes.device_class === deviceClass)
  return (of('power') ?? null)?.id ?? null
}

/** The action a key of two states follows, as the engine picks it: the first one with a state,
 * of OBS or of Home Assistant. */
export function followedAction(first: Action[], second: Action[]): Action | null {
  for (const action of [...first, ...second]) {
    if ((action.type === 'obs' && obsFollowedState(action.command)) || (action.type === 'home_assistant' && followedState(action.command))) {
      return action
    }
  }
  return null
}

export function sameCommand(a: HassCommand, b: HassCommand) {
  return JSON.stringify(a) === JSON.stringify(b)
}

/** Gives the key a second state that follows the command: a copy of the first look in the
 * color of the state, doing the same. */
export function followInKey(draft: KeyDraft, command: HassCommand, server: string) {
  const state = followedState(command)
  if (!state || draft.second) {
    return
  }
  const { background, from, to, angle, image, text, untouched, widget, moreWidgets } = draft
  const second = JSON.parse(JSON.stringify({ background, from, to, angle, image, text, untouched, widget, moreWidgets })) as Omit<StateDraft, 'actions'>
  draft.second = {
    ...second,
    background: 'solid',
    from: state.color,
    actions: [{ type: 'home_assistant', server, command: JSON.parse(JSON.stringify(command)) as HassCommand }],
  }
}

/** A new command of `kind`, about `entity` if it takes one. */
export function newCommand(kind: HassKind, entity = ''): HassCommand {
  switch (kind) {
    case 'switch':
      return { kind, entity, mode: 'toggle' }
    case 'run':
    case 'color':
      return kind === 'run' ? { kind, entity } : { kind, entity, color: { r: 255, g: 180, b: 80 } }
    case 'level':
      return { kind, entity, level: levelsOf(entity)[0] ?? 'brightness', change: { type: 'step', by: 10 } }
    case 'choose':
      return { kind, entity, option: optionsOf(entity)[0] ?? 'option', choice: { type: 'next' } }
    case 'media':
      return { kind, entity, control: 'play_pause' }
    case 'alarm':
      return { kind, entity, mode: 'arm_away' }
    case 'call':
      return { kind, action: '', target: { entities: entity ? [entity] : [], devices: [], areas: [], floors: [], labels: [] }, data: {} }
    case 'assist':
      return { kind, text: '', agent: '' }
    case 'event':
      return { kind, event: '', data: {} }
    case 'show':
      return { kind, entity }
    case 'tile':
      return { kind, entity, press: 'auto' }
  }
}

/** The connection to Home Assistant in a few words. */
export function hassStatusText(info: HassInfo): string {
  const status = info.status
  switch (status.state) {
    case 'off':
      return t('actions.hass.status.off')
    case 'connecting':
      return t('actions.hass.status.connecting')
    case 'connected':
      return status.location
        ? t('actions.hass.status.connectedTo', { location: status.location, version: status.version })
        : t('actions.hass.status.connected', { version: status.version })
    case 'failed':
      return t(`actions.hass.status.${{
        no_address: 'noAddress',
        unreachable: 'unreachable',
        not_home_assistant: 'notHomeAssistant',
        certificate: 'certificate',
        token_required: 'tokenRequired',
        wrong_token: 'wrongToken',
        other: 'failed',
      }[status.reason]}`)
  }
}
