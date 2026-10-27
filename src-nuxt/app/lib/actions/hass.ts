import type { Action, HassCommand, HassEntityInfo, HassInfo, HassLevel, HassOption } from '~/types/engine'
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
  const entityKinds: HassKind[] = ['switch', 'run', 'level', 'choose', 'media', 'color', 'alarm']
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
    default:
      return null
  }
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
  const { background, from, to, angle, image, text, untouched, widget } = draft
  const second = JSON.parse(JSON.stringify({ background, from, to, angle, image, text, untouched, widget })) as Omit<StateDraft, 'actions'>
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
