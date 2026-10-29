import type { Component } from 'vue'
import { shallowRef } from 'vue'
import {
  AppWindow,
  ArrowLeft,
  ArrowRight,
  ArrowRightLeft,
  AudioLines,
  Bookmark,
  Camera,
  ChevronLeft,
  ChevronRight,
  Circle,
  CircleStop,
  CircleX,
  Clapperboard,
  Eye,
  Film,
  Globe,
  Hash,
  House,
  IdCard,
  Keyboard,
  Library,
  Lightbulb,
  ListChecks,
  MessageSquare,
  Monitor,
  Music,
  Palette,
  Pause,
  Play,
  Power,
  Radio,
  Repeat,
  Repeat2,
  Save,
  ShieldCheck,
  Shuffle,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Sparkles,
  Sun,
  SunDim,
  Thermometer,
  Timer,
  Type,
  UserRound,
  Video,
  Volume1,
  Volume2,
  VolumeX,
  Wand,
  Webhook,
  Workflow,
  Zap,
} from '@lucide/vue'
import { newHttpRequest } from '~/lib/actions/http'
import { i18n, t } from '~/lib/i18n'
import type { Action, HassCommand, MediaKey, ObsCommand } from '~/types/engine'
import { newCommand } from '~/lib/actions/hass'
import { NO_SERVER } from '~/lib/integrations'
import { os } from '~/lib/os'

/** Windows lets any app press keys. */
const KEYS_NEED_PERMISSION = os !== 'windows'

/** The built in sections, and one per installed plugin, as `plugin:<id>`. */
export type SectionId = 'system' | 'media' | 'pages' | 'sequence' | 'sound' | 'obs' | 'home_assistant' | 'lights' | 'tc002' | `plugin:${string}`

export interface Section {
  id: SectionId
  name: string
  description: string
}

export const SECTIONS: Section[] = (['system', 'media', 'pages', 'sequence', 'sound', 'obs', 'home_assistant', 'lights', 'tc002'] as const).map(id => ({
  id,
  get name() {
    return t(`actions.catalog.sections.${id}.name`)
  },
  get description() {
    return t(`actions.catalog.sections.${id}.description`)
  },
}))

/** What a new action of an entry starts with; some need to know the page they are on. */
export interface NewActionContext {
  otherPages: string[]
  profiles: string[]
}

export type KeySetup = 'switch_hotkey' | 'two_states' | 'page_indicator'

export interface CatalogEntry {
  id: string
  section: SectionId
  name: string
  description: string
  icon: Component
  /** An image instead of the icon: those of plugins. */
  image?: string
  /** An action of a plugin, which draws the key it is on. */
  plugin?: boolean
  keywords?: string[]
  /** Not built yet: shown so nothing of Ulanzi Studio is missing, but cannot be added. */
  soon?: string
  /** Presses keys or types, which macOS only allows with Accessibility and Linux with a udev rule. */
  accessibility?: boolean
  /** Moves the dock to another page or profile; a key may do this once. */
  navigates?: boolean
  /** Can be one of the picks of a random action. */
  random?: boolean
  /** Has too many fields for the list: it is set up in a dialog and the list shows a summary. */
  modal?: boolean
  /** Sets up the key instead of adding an action: two states or a page indicator. */
  setup?: KeySetup
  create?: (context: NewActionContext) => Action
  /** Whether an existing action is one of this entry; by type unless told otherwise. */
  matches?: (action: Action) => boolean
}

type Texts = 'name' | 'description' | 'keywords' | 'soon'

interface TextOptions {
  /** Words found in any language, such as brand names. */
  keywords?: string[]
  /** A message of keywords shared by several entries, instead of the entry's own. */
  keywordsFrom?: string
  /** Not built yet for want of hardware to test it. */
  needsHardware?: boolean
}

function camel(text: string) {
  return text.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
}

function hasMessage(key: string) {
  return (i18n.global as unknown as { te: (key: string) => boolean }).te(key)
}

/** The entry's texts, read in the language shown now from the messages named after its id. */
function localized(entry: Omit<CatalogEntry, Texts>, options: TextOptions = {}): CatalogEntry {
  const key = `actions.catalog.entries.${entry.id.split(':').map(camel).join('.')}`
  return Object.assign({
    get name() {
      return t(`${key}.name`)
    },
    get description() {
      return t(`${key}.description`)
    },
    get keywords() {
      const message = options.keywordsFrom ?? `${key}.keywords`
      const words = hasMessage(message) ? t(message).split(',').map(word => word.trim()) : []
      return [...(options.keywords ?? []), ...words]
    },
    get soon() {
      return options.needsHardware ? t('actions.catalog.needsHardware') : undefined
    },
  }, entry)
}

function media(key: MediaKey, icon: Component): CatalogEntry {
  return localized({
    id: `media:${key}`,
    section: 'media',
    icon,
    accessibility: KEYS_NEED_PERMISSION,
    random: true,
    create: () => ({ type: 'media', key }),
    matches: action => action.type === 'media' && action.key === key,
  })
}

/** `kinds` are the commands the entry stands for, when it is more than the one it creates. */
function obs(id: string, icon: Component, create: () => ObsCommand, kinds?: ObsCommand['kind'][]): CatalogEntry {
  const matching = kinds ?? [create().kind]
  return localized({
    id: `obs:${id}`,
    section: 'obs',
    icon,
    random: true,
    modal: true,
    create: () => ({ type: 'obs', server: NO_SERVER, command: create() }),
    matches: action => action.type === 'obs' && matching.includes(action.command.kind),
  }, { keywords: ['obs'] })
}

/** An entry of Home Assistant: a kind of command, set up in a dialog with how the key shows it. */
function hass(kind: HassCommand['kind'], icon: Component): CatalogEntry {
  return localized({
    id: `home_assistant:${kind}`,
    section: 'home_assistant',
    icon,
    random: true,
    modal: true,
    create: () => ({ type: 'home_assistant', server: NO_SERVER, command: newCommand(kind) }),
    matches: action => action.type === 'home_assistant' && action.command.kind === kind,
  }, { keywords: ['home assistant', 'hass', 'ha'], keywordsFrom: 'actions.catalog.hassKeywords' })
}

function lights(id: string, icon: Component): CatalogEntry {
  return localized({ id: `lights:${id}`, section: 'lights', icon }, { keywordsFrom: 'actions.catalog.lightsKeywords', needsHardware: true })
}

export const CATALOG: CatalogEntry[] = [
  localized({
    id: 'open',
    section: 'system',
    icon: AppWindow,
    random: true,
    create: () => ({ type: 'open', path: '' }),
  }),
  localized({
    id: 'close_app',
    section: 'system',
    icon: CircleX,
    random: true,
    create: () => ({ type: 'close_app', app: '' }),
  }),
  localized({
    id: 'open_url',
    section: 'system',
    icon: Globe,
    random: true,
    create: () => ({ type: 'open_url', url: '' }),
  }),
  localized({
    id: 'http',
    section: 'system',
    icon: Webhook,
    random: true,
    modal: true,
    create: () => ({ type: 'http', request: newHttpRequest() }),
  }),
  localized({
    id: 'type_text',
    section: 'system',
    icon: Type,
    accessibility: KEYS_NEED_PERMISSION,
    random: true,
    create: () => ({ type: 'type_text', text: '', enter: false, paste: false }),
  }),
  localized({
    id: 'hotkey',
    section: 'system',
    icon: Keyboard,
    accessibility: KEYS_NEED_PERMISSION,
    random: true,
    create: () => ({ type: 'hotkey', keys: { key: '', command: false, shift: false, option: false, control: false } }),
  }),
  localized({
    id: 'switch_hotkey',
    section: 'system',
    icon: ArrowRightLeft,
    setup: 'switch_hotkey',
  }),
  media('play_pause', Play),
  media('next', SkipForward),
  media('previous', SkipBack),
  media('volume_up', Volume2),
  media('volume_down', Volume1),
  media('mute', VolumeX),
  media('brightness_up', Sun),
  media('brightness_down', SunDim),
  localized({
    id: 'open_page',
    section: 'pages',
    icon: ArrowRight,
    navigates: true,
    create: ({ otherPages }) => ({ type: 'open_page', page: otherPages[0] ?? '' }),
  }),
  localized({
    id: 'back',
    section: 'pages',
    icon: ArrowLeft,
    navigates: true,
    create: () => ({ type: 'back' }),
  }),
  localized({
    id: 'home',
    section: 'pages',
    icon: House,
    navigates: true,
    create: () => ({ type: 'home' }),
  }),
  localized({
    id: 'previous_page',
    section: 'pages',
    icon: ChevronLeft,
    navigates: true,
    create: () => ({ type: 'previous_page' }),
  }),
  localized({
    id: 'next_page',
    section: 'pages',
    icon: ChevronRight,
    navigates: true,
    create: () => ({ type: 'next_page' }),
  }),
  localized({
    id: 'switch_profile',
    section: 'pages',
    icon: UserRound,
    navigates: true,
    create: ({ profiles }) => ({ type: 'switch_profile', profile: profiles[0] ?? '' }),
  }),
  localized({
    id: 'page_indicator',
    section: 'pages',
    icon: Hash,
    setup: 'page_indicator',
  }),
  localized({
    id: 'delay',
    section: 'sequence',
    icon: Timer,
    random: false,
    create: () => ({ type: 'delay', ms: 500 }),
  }),
  localized({
    id: 'random',
    section: 'sequence',
    icon: Shuffle,
    create: () => ({ type: 'random', actions: [] }),
  }),
  localized({
    id: 'toggle_sequences',
    section: 'sequence',
    icon: Repeat2,
    setup: 'two_states',
  }),
  localized({
    id: 'play_sound',
    section: 'sound',
    icon: Music,
    random: true,
    create: () => ({ type: 'play_sound', sound: '', name: '', volume: 80, mode: 'restart', device: null }),
  }),
  localized({
    id: 'stop_sounds',
    section: 'sound',
    icon: CircleStop,
    random: true,
    create: () => ({ type: 'stop_sounds' }),
  }),
  obs('record', Circle, () => ({ kind: 'record', mode: 'toggle' })),
  obs('record_pause', Pause, () => ({ kind: 'record_pause', mode: 'toggle' })),
  obs('stream', Radio, () => ({ kind: 'stream', mode: 'toggle' })),
  obs('virtual_cam', Video, () => ({ kind: 'virtual_cam', mode: 'toggle' })),
  obs('replay_buffer', Repeat, () => ({ kind: 'replay_buffer', mode: 'toggle' })),
  obs('replay_save', Save, () => ({ kind: 'save_replay' })),
  obs('studio_mode', Monitor, () => ({ kind: 'studio_mode', mode: 'toggle' })),
  obs('transition', Clapperboard, () => ({ kind: 'transition' })),
  obs('scene_transition', Wand, () => ({ kind: 'set_transition', transition: '', duration_ms: null })),
  obs('screenshot', Camera, () => ({ kind: 'screenshot', source: '', folder: '' })),
  obs('scene_collection', Library, () => ({ kind: 'scene_collection', collection: '' })),
  obs('profile', IdCard, () => ({ kind: 'profile', profile: '' })),
  obs('scene', Film, () => ({ kind: 'scene', scene: '', preview: false })),
  obs('source_visibility', Eye, () => ({ kind: 'source_visibility', scene: '', source: '', mode: 'toggle' })),
  obs('chapter', Bookmark, () => ({ kind: 'chapter', name: '' })),
  obs('filter', SlidersHorizontal, () => ({ kind: 'filter', source: '', filter: '', mode: 'toggle' })),
  obs('media_source', Play, () => ({ kind: 'media', input: '', control: 'play_pause' })),
  obs('audio_mixer', AudioLines, () => ({ kind: 'mute', input: '', mode: 'toggle' }), ['mute', 'volume']),
  hass('tile', Sparkles),
  hass('switch', Power),
  hass('run', Play),
  hass('level', SlidersHorizontal),
  hass('choose', ListChecks),
  hass('media', Music),
  hass('color', Palette),
  hass('alarm', ShieldCheck),
  hass('call', Workflow),
  hass('assist', MessageSquare),
  hass('event', Zap),
  hass('show', Eye),
  lights('switch', Power),
  lights('brightness', Lightbulb),
  lights('temperature', Thermometer),
  lights('color', Palette),
  lights('effect', Sparkles),
  lights('temperature_brightness', Thermometer),
  lights('effect_brightness', Sparkles),
  localized({ id: 'tc002:brightness', section: 'tc002', icon: Sun }, { needsHardware: true }),
  localized({ id: 'tc002:app', section: 'tc002', icon: AppWindow }, { needsHardware: true }),
]

/** Sections and actions of the plugins installed, kept up to date by `usePlugins`. */
export const pluginSections = shallowRef<Section[]>([])
export const pluginEntries = shallowRef<CatalogEntry[]>([])

export function allSections(): Section[] {
  return [...SECTIONS, ...pluginSections.value]
}

export function allEntries(): CatalogEntry[] {
  return [...CATALOG, ...pluginEntries.value]
}

export function entryFor(action: Action): CatalogEntry | undefined {
  return allEntries().find(entry => (!entry.soon || entry.plugin) && (entry.matches ? entry.matches(action) : entry.id === action.type))
}

/** Lowercase and without accents, so "musica" finds "Música". */
export function normalize(text: string) {
  return text.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase()
}

export function searchText(entry: CatalogEntry) {
  const section = allSections().find(s => s.id === entry.section)?.name ?? ''
  return normalize([entry.name, entry.description, section, ...(entry.keywords ?? [])].join(' '))
}

/** Every word of the query must appear somewhere in the entry. */
export function matchesQuery(entry: CatalogEntry, query: string) {
  const words = normalize(query).split(/\s+/).filter(Boolean)
  const haystack = searchText(entry)
  return words.every(word => haystack.includes(word))
}
