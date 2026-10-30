import type { DeepReadonly } from 'vue'
import { commands } from '~/ipc/bindings'
import type { Action, Appearance, Background, Crop, Fit, Gesture, Key, LayerConfig, VerticalPosition, Widget } from '~/types/engine'
import type { KeyRef } from '~/composables/useWorkspace'
import { isNavigation } from '~/composables/usePages'
import type { PickedIcon } from '~/composables/useIconCatalog'
import { fromHex, toHex } from '~/lib/color'
import { iconSvg } from '~/lib/iconify'
import { httpError } from '~/lib/actions/http'
import { hassError } from '~/lib/actions/hass'
import { obsError } from '~/lib/actions/obs'
import { NO_SERVER } from '~/lib/integrations'
import type { NewKeyStyle } from '~/lib/new-key'
// Relative, not `~`: the unit tests import this file without Nuxt's alias.
import { t } from '../lib/i18n'

export interface ImageDraft {
  /** What the key shows: the original image, or an edited copy of it. */
  asset: string
  /** The original image, edited again from scratch each time. */
  source: string
  /** How `source` was framed into `asset`; null while the image is not edited. */
  crop: Crop | null
  fit: Fit
  inset: number
  /** Hex color applied to icons drawn with currentColor; null keeps the image as it is. */
  tint: string | null
  icon: string | null
}

export interface TextDraft {
  content: string
  fontFamily: string
  fontWeight: number
  size: number
  color: string
  position: VerticalPosition
}

/** Flat, form-friendly version of one look of a key and what it does. Layers the editor does not
 * handle are kept as they are. */
export interface StateDraft {
  background: 'solid' | 'gradient'
  from: string
  to: string
  angle: number
  image: ImageDraft | null
  text: TextDraft | null
  untouched: LayerConfig[]
  actions: Action[]
  /** Live content, drawn between the image and the text. */
  widget: Widget | null
  /** Shown in turn after `widget`, one more on each press. */
  moreWidgets: Widget[]
}

/** A whole key: its first state, gestures, and the second state of a key of two. */
export interface KeyDraft extends StateDraft {
  doublePress: Action[]
  longPress: Action[]
  second: StateDraft | null
}

const WEB_URL = /^https?:\/\/\S+$/

export function emptyDraft(style: NewKeyStyle): KeyDraft {
  return {
    background: 'solid',
    from: toHex(style.background),
    to: '#3f3f46',
    angle: 90,
    image: null,
    text: {
      content: t('editor.newKey.text'),
      fontFamily: style.font_family,
      fontWeight: style.font_weight,
      size: style.text_size,
      color: toHex(style.text_color),
      position: style.text_position,
    },
    untouched: [],
    actions: [],
    widget: null,
    moreWidgets: [],
    doublePress: [],
    longPress: [],
    second: null,
  }
}

function clone<T>(value: DeepReadonly<T> | T): T {
  return structuredClone(toRaw(value)) as T
}

function stateFrom(appearance: DeepReadonly<Appearance>, actions: DeepReadonly<Action[]>): StateDraft {
  const bg = appearance.background
  const state: StateDraft = {
    background: bg.type === 'solid' ? 'solid' : 'gradient',
    from: toHex(bg.type === 'solid' ? bg.color : bg.from),
    to: toHex(bg.type === 'solid' ? bg.color : bg.to),
    angle: bg.type === 'solid' ? 90 : bg.angle,
    image: null,
    text: null,
    untouched: [],
    actions: clone<Action[]>(actions),
    widget: null,
    moreWidgets: [],
  }
  for (const layer of appearance.layers) {
    if (layer.type === 'image' && !state.image) {
      state.image = {
        asset: layer.asset,
        fit: layer.fit,
        inset: layer.inset,
        tint: layer.tint ? toHex(layer.tint) : null,
        icon: layer.icon ?? null,
        source: layer.source ?? layer.asset,
        crop: layer.crop ? { ...layer.crop } : null,
      }
    }
    else if (layer.type === 'text' && !state.text) {
      state.text = {
        content: layer.content,
        fontFamily: layer.font_family,
        fontWeight: layer.font_weight,
        size: layer.size,
        color: toHex(layer.color),
        position: layer.position,
      }
    }
    else if (layer.type === 'widget') {
      const widget = clone<Widget>(layer.widget)
      if (state.widget) {
        state.moreWidgets.push(widget)
      }
      else {
        state.widget = widget
      }
    }
    else {
      state.untouched.push(clone<LayerConfig>(layer))
    }
  }
  return state
}

export function draftFromKey(key: DeepReadonly<Key>): KeyDraft {
  return {
    ...stateFrom(key.appearance, key.actions),
    doublePress: clone<Action[]>(key.double_press),
    longPress: clone<Action[]>(key.long_press),
    second: key.second_state ? stateFrom(key.second_state.appearance, key.second_state.actions) : null,
  }
}

/** The live content of a look, in the order a press goes through it. */
export function widgetsOf(state: StateDraft): Widget[] {
  return state.widget ? [state.widget, ...state.moreWidgets] : []
}

/** The look of one state as the engine stores it; with `onlyWidget`, holding just that one of its
 * widgets, so a preview shows it whatever turn the key is on. */
export function appearanceFrom(state: StateDraft, onlyWidget?: number): Appearance {
  const background: Background = state.background === 'solid'
    ? { type: 'solid', color: fromHex(state.from) }
    : { type: 'linear_gradient', from: fromHex(state.from), to: fromHex(state.to), angle: state.angle }
  const layers: LayerConfig[] = []
  if (state.image) {
    layers.push({
      type: 'image',
      asset: state.image.asset,
      fit: state.image.fit,
      inset: state.image.inset,
      tint: state.image.tint ? fromHex(state.image.tint) : null,
      icon: state.image.icon,
      source: state.image.crop ? state.image.source : null,
      crop: state.image.crop ? { ...state.image.crop } : null,
    })
  }
  const widgets = widgetsOf(state)
  const shown = onlyWidget === undefined ? widgets : widgets.slice(onlyWidget, onlyWidget + 1)
  for (const widget of shown.length ? shown : widgets) {
    // Read through the proxy, not toRaw: whoever compares drafts must see changes inside it.
    layers.push({ type: 'widget', widget: JSON.parse(JSON.stringify(widget)) as Widget })
  }
  if (state.text && state.text.content.trim()) {
    layers.push({
      type: 'text',
      content: state.text.content,
      font_family: state.text.fontFamily,
      font_weight: state.text.fontWeight,
      size: state.text.size,
      color: fromHex(state.text.color),
      position: state.text.position,
      outline: null,
    })
  }
  layers.push(...state.untouched)
  return { background, layers }
}

function cleanActions(actions: Action[]): Action[] {
  return actions.map(a => (a.type === 'open_url' ? { ...a, url: a.url.trim() } : { ...a }))
}

export function keyFromDraft(draft: KeyDraft): Key {
  return {
    appearance: appearanceFrom(draft),
    actions: cleanActions(draft.actions),
    double_press: draft.second ? [] : cleanActions(draft.doublePress),
    long_press: draft.second ? [] : cleanActions(draft.longPress),
    second_state: draft.second ? { appearance: appearanceFrom(draft.second), actions: cleanActions(draft.second.actions) } : null,
  }
}

export function urlError(url: string): string | null {
  const value = url.trim()
  if (!value) {
    return t('editor.validation.urlMissing')
  }
  return WEB_URL.test(value) ? null : t('editor.validation.urlScheme')
}

/** `servers` are the ids of the servers of OBS and Home Assistant; without them, servers are not checked. */
export function actionsError(actions: Action[], servers?: string[]): string | null {
  if (actions.filter(isNavigation).length > 1) {
    return t('editor.validation.oneNavigation')
  }
  for (const action of actions) {
    const problem = actionError(action, servers)
    if (problem) {
      return problem
    }
  }
  return null
}

/** What is missing for the whole key to be saved: every list follows the rules of a list, and
 * only the simple press can lead to a page of the canvas. */
export function keyDraftError(draft: KeyDraft, servers?: string[]): string | null {
  const others = draft.second ? [draft.second.actions] : [draft.doublePress, draft.longPress]
  if (others.some(list => list.some(a => a.type === 'open_page'))) {
    return t('editor.validation.pageOnlyOnPress')
  }
  return [draft.actions, ...others].map(list => actionsError(list, servers)).find(Boolean) ?? null
}

/** What is missing for an action to be saved, the same checks the engine makes. */
function actionError(action: Action, servers?: string[]): string | null {
  if ((action.type === 'obs' || action.type === 'home_assistant') && servers && !servers.includes(action.server ?? NO_SERVER)) {
    return t('integrations.errors.choose')
  }
  switch (action.type) {
    case 'open_url':
      return urlError(action.url)
    case 'open':
      return action.path ? null : t('editor.validation.open')
    case 'close_app':
      return action.app ? null : t('editor.validation.closeApp')
    case 'type_text':
      return action.text ? null : t('editor.validation.typeText')
    case 'hotkey':
      return action.keys.key ? null : t('editor.validation.hotkey')
    case 'open_page':
      return action.page ? null : t('editor.validation.openPage')
    case 'switch_profile':
      return action.profile ? null : t('editor.validation.switchProfile')
    case 'delay':
      return action.ms > 60000 ? t('editor.validation.delay') : null
    case 'play_sound':
      return action.sound ? null : t('editor.validation.playSound')
    case 'random':
      if (!action.actions.length) {
        return t('editor.validation.random')
      }
      return action.actions.map(a => actionError(a, servers)).find(Boolean) ?? null
    case 'obs':
      return obsError(action.command)
    case 'http':
      return httpError(action.request)
    case 'home_assistant':
      return hassError(action.command)
    default:
      return null
  }
}

/** Quiet time after the last change before it is saved. */
const AUTOSAVE_MS = 500
const LIVE_MS = 1000

export type SaveStatus = 'saved' | 'pending' | 'saving' | 'error'

/** A key as a string that ignores field order and empty optional fields, to compare two keys. */
export function canonical(value: unknown): string {
  return JSON.stringify(clean(value))
}

function clean(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(clean)
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value)
      .filter(([key, v]) => v !== null && v !== undefined && !(key === 'waypoints' && Array.isArray(v) && !v.length))
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, v]) => [key, clean(v)]))
  }
  return value
}

/** Editing state for one key: draft, live preview drawn by the renderer, saved as it changes. */
export function useKeyEditor(target: Ref<KeyRef | null>) {
  const { snapshot, upload, previewUrl, redraws } = useEngine()

  const stored = computed(() => {
    const ref = target.value
    if (!ref) {
      return null
    }
    return snapshot.value?.pages.find(p => p.id === ref.page)?.keys[ref.index] ?? null
  })
  const draft = ref<KeyDraft | null>(null)
  const preview = ref<string | null>(null)
  const status = ref<SaveStatus>('saved')
  const error = ref<string | null>(null)

  /** The key as last loaded or saved, so what arrives from the engine can be told apart. */
  const known = ref(canonical(null))
  /** Saves on their way: their echo from the engine is not a change made elsewhere. */
  let sent: string[] = []
  let timer: ReturnType<typeof setTimeout> | undefined

  function load() {
    clearTimeout(timer)
    draft.value = stored.value ? draftFromKey(stored.value) : null
    known.value = canonical(stored.value)
    status.value = 'saved'
    error.value = null
  }

  const dirty = computed(() => Boolean(draft.value) && canonical(keyFromDraft(draft.value!)) !== known.value)
  const servers = computed(() => (snapshot.value?.integrations ?? []).map(server => server.id))
  const invalid = computed(() => (draft.value ? keyDraftError(draft.value, servers.value) : null))
  /** Which state the inspector shows and edits, for a key of two. */
  const editing = ref<0 | 1>(0)
  /** Which of the widgets of that state the inspector edits, and so the preview shows. */
  const shownWidget = ref(0)

  async function save(ref: KeyRef, key: Key) {
    const text = canonical(key)
    const before = known.value
    known.value = text
    sent.push(text)
    status.value = 'saving'
    error.value = null
    try {
      await call(commands.setKey(ref.page, ref.index, key))
      if (status.value === 'saving') {
        status.value = 'saved'
      }
    }
    catch {
      sent = sent.filter(s => s !== text)
      known.value = before
      status.value = 'error'
      error.value = t('editor.errors.save')
    }
  }

  /** Saves now what is waiting, for the key being edited until now. */
  function flush(ref = target.value, value = draft.value) {
    clearTimeout(timer)
    if (status.value !== 'pending' || !ref || !value || keyDraftError(value, servers.value)) {
      return
    }
    save(ref, keyFromDraft(value))
  }

  watch(() => target.value && `${target.value.page}:${target.value.index}`, (_, previous) => {
    if (previous) {
      const [page, index] = previous.split(':')
      flush({ page: page!, index: Number(index) }, draft.value)
    }
    sent = []
    load()
  }, { immediate: true })

  watch(stored, (key) => {
    const incoming = canonical(key)
    const echo = sent.indexOf(incoming)
    if (echo !== -1) {
      sent = sent.slice(echo + 1)
      return
    }
    if (incoming === known.value) {
      return
    }
    if (status.value === 'pending' && key && draft.value) {
      // A line drawn on the canvas while this key has changes waiting to be saved.
      known.value = incoming
      draft.value.actions = structuredClone(toRaw(key.actions)) as Action[]
      return
    }
    load()
  })

  watch(draft, (value) => {
    clearTimeout(timer)
    if (!value || !dirty.value) {
      if (status.value === 'pending') {
        status.value = 'saved'
      }
      return
    }
    status.value = 'pending'
    if (!invalid.value) {
      timer = setTimeout(flush, AUTOSAVE_MS)
    }
  }, { deep: true })

  // A server added from an action is picked before the snapshot brings it.
  watch(invalid, (problem) => {
    if (!problem && status.value === 'pending') {
      clearTimeout(timer)
      timer = setTimeout(flush, AUTOSAVE_MS)
    }
  })

  /** Undo first drops changes not saved yet, if there are any. */
  function discardPending() {
    if (status.value !== 'pending') {
      return false
    }
    load()
    return true
  }
  const forget = registerPendingEdit(discardPending)

  onBeforeUnmount(() => {
    flush()
    forget()
  })

  /** The state the inspector edits: the first, or the second of a key of two. */
  const state = computed<StateDraft | null>(() => {
    const value = draft.value
    return value && editing.value === 1 && value.second ? value.second : value
  })

  watch(() => target.value && `${target.value.page}:${target.value.index}`, () => {
    editing.value = 0
  })
  watch([target, editing], () => {
    shownWidget.value = 0
  })
  watch(() => draft.value?.second, (second) => {
    if (!second) {
      editing.value = 0
    }
  })

  let previewTimer: ReturnType<typeof setTimeout> | undefined
  /** A plugin redraws its keys by itself; the preview follows. */
  const redrawn = computed(() => (target.value ? redraws(target.value.page, target.value.index) : 0))
  watch([state, editing, redrawn, shownWidget], ([value]) => {
    clearTimeout(previewTimer)
    if (!value || !target.value) {
      preview.value = null
      return
    }
    const { page, index } = target.value
    previewTimer = setTimeout(() => {
      preview.value = `${previewUrl(index, appearanceFrom(value, shownWidget.value), page)}${redrawn.value ? `&v=${redrawn.value}` : ''}`
    }, 120)
  }, { deep: true, immediate: true })

  /** A look with live content is drawn again every second, so the preview keeps up. */
  let liveTimer: ReturnType<typeof setInterval> | undefined
  watch(() => Boolean(state.value?.widget), (live) => {
    clearInterval(liveTimer)
    if (live) {
      liveTimer = setInterval(() => {
        const value = state.value
        if (value && target.value) {
          preview.value = `${previewUrl(target.value.index, appearanceFrom(value, shownWidget.value), target.value.page)}&live=${Date.now()}`
        }
      }, LIVE_MS)
    }
  }, { immediate: true })
  onBeforeUnmount(() => clearInterval(liveTimer))

  function create() {
    draft.value = emptyDraft(snapshot.value!.preferences.new_keys as NewKeyStyle)
  }

  async function clear() {
    const key = target.value
    if (!key) {
      return
    }
    clearTimeout(timer)
    known.value = canonical(null)
    status.value = 'saved'
    draft.value = null
    await call(commands.clearKey(key.page, key.index))
  }

  /** Runs the saved key as if it had been pressed on the dock. */
  async function tryKey(gesture: Gesture = 'press') {
    const key = target.value
    if (!key) {
      return
    }
    error.value = null
    try {
      await call(commands.pressKey(key.page, key.index, gesture))
    }
    catch {
      error.value = t('editor.errors.try')
    }
  }

  function setImage(image: ImageDraft) {
    if (!state.value) {
      return
    }
    state.value.image = image
    if (state.value.text?.position === 'middle') {
      state.value.text.position = 'bottom'
    }
  }

  async function uploadImage(file: File) {
    error.value = null
    try {
      const id = await upload(file, file.type)
      setImage({ asset: id, source: id, crop: null, fit: 'cover', inset: 0, tint: null, icon: null })
    }
    catch {
      error.value = t('editor.errors.upload')
    }
  }

  async function pickIcon({ icon, multicolor }: PickedIcon) {
    error.value = null
    try {
      const asset = await upload(await iconSvg(icon), 'image/svg+xml')
      const tint = multicolor ? null : (state.value?.text?.color ?? '#ffffff')
      setImage({ asset, source: asset, crop: null, fit: 'contain', inset: 0.22, tint, icon })
    }
    catch {
      error.value = t('editor.errors.icon')
    }
  }

  return { stored, draft, state, editing, shownWidget, preview, dirty, invalid, status, error, create, clear, uploadImage, pickIcon, tryKey }
}
