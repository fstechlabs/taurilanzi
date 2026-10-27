import { getName, getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { commands, events } from '~/ipc/bindings'
import type { Appearance, ErrorKind, IpcError, Scene, Snapshot } from '~/types/engine'
import { hasWidget } from '~/lib/widgets'

type Status = 'connecting' | 'online' | 'offline'
type Result<T> = { status: 'ok', data: T } | { status: 'error', error: IpcError }

const RETRY_MS = 1000
/** How often images of keys with live content load again. */
const LIVE_MS = 1000
/** Windows serves custom schemes over http. */
const IMAGES = import.meta.client && navigator.userAgent.includes('Windows') ? 'http://deck.localhost' : 'deck://localhost'

export class EngineError extends Error {
  constructor(readonly kind: ErrorKind, message: string) {
    super(message)
  }
}

/** Unwraps what a command returns, throwing what the engine rejected it with. */
export async function call<T>(pending: Promise<Result<T>>): Promise<T> {
  const result = await pending
  if (result.status === 'error') {
    throw new EngineError(result.error.kind, result.error.message)
  }
  return result.data
}

/** Keys a plugin redrew since the app opened, and how many times. */
const redrawn = ref(new Map<string, number>())
const snapshot = ref<Snapshot | null>(null)
const status = ref<Status>('connecting')
const pressed = ref(new Set<number>())
const version = ref<string | null>(null)
/** Development builds have a name of their own, so they are never mistaken for the released app. */
const name = ref<string | null>(null)
let started = false
const liveTick = ref(0)
let liveTimer: ReturnType<typeof setInterval> | undefined

/** Whether a key of the snapshot shows live content in the state it is in now. */
function isLive(value: Snapshot, page: string, index: number, second: boolean) {
  const key = value.pages.find(p => p.id === page)?.keys[index]
  const appearance = second && key?.second_state ? key.second_state.appearance : key?.appearance
  return Boolean(appearance && hasWidget(appearance))
}

/** Ticks while any key shows live content, so only then do their images load again. */
function watchLive() {
  watch(() => snapshot.value?.pages.some(page => Object.values(page.keys).some(key =>
    key && (hasWidget(key.appearance) || (key.second_state && hasWidget(key.second_state.appearance))))), (live) => {
    clearInterval(liveTimer)
    if (live) {
      liveTimer = setInterval(() => liveTick.value++, LIVE_MS)
    }
  }, { immediate: true })
}

async function refresh() {
  snapshot.value = await call(commands.getState())
}

async function connect() {
  await events.stateChanged.listen((event) => {
    snapshot.value = event.payload
  })
  await events.keyImageChanged.listen(({ payload }) => {
    const id = `${payload.page}:${payload.index}`
    redrawn.value = new Map(redrawn.value).set(id, (redrawn.value.get(id) ?? 0) + 1)
  })
  await events.keyChanged.listen(({ payload }) => {
    const next = new Set(pressed.value)
    if (payload.pressed) {
      next.add(payload.index)
    }
    else {
      next.delete(payload.index)
    }
    pressed.value = next
  })
  version.value = await getVersion()
  name.value = await getName()
  while (!snapshot.value) {
    try {
      await refresh()
      status.value = 'online'
    }
    catch {
      status.value = 'offline'
      await new Promise(resolve => setTimeout(resolve, RETRY_MS))
    }
  }
}

/** Stores an image and returns its id. The bytes travel as they are, not as JSON. */
async function upload(body: Blob | string, type: string) {
  const bytes = typeof body === 'string' ? new TextEncoder().encode(body) : new Uint8Array(await body.arrayBuffer())
  try {
    return await invoke<string>('put_asset', bytes, { headers: { 'Asset-Type': type } })
  }
  catch (err) {
    const error = err as IpcError
    throw new EngineError(error.kind ?? 'failed', error.message ?? String(err))
  }
}

function assetUrl(id: string) {
  return `${IMAGES}/asset/${id}`
}

async function assetBlob(id: string) {
  const response = await fetch(assetUrl(id))
  if (!response.ok) {
    throw new EngineError('not_found', `asset ${id} could not be loaded`)
  }
  return await response.blob()
}

/** The engine's state, kept up to date by its events, and what the UI can ask of it. */
export function useEngine() {
  if (!started && import.meta.client) {
    started = true
    connect()
    watchLive()
  }

  /** A saved key as the dock shows it; the revision and the state make it load again after a
   * change, and keys with live content load again every second. */
  function keyImageUrl(page: string, index: number) {
    const value = snapshot.value
    if (!value) {
      return null
    }
    const second = value.second_states.some(s => s.page === page && s.index === index)
    const live = isLive(value, page, index, second) ? `&live=${liveTick.value}` : ''
    const drawn = redrawn.value.get(`${page}:${index}`)
    return `${IMAGES}/key/${page}/${index}?rev=${value.revision}${second ? '&state=2' : ''}${live}${drawn ? `&v=${drawn}` : ''}`
  }

  /** How many times a plugin redrew a key, to load its image again. */
  function redraws(page: string, index: number) {
    return redrawn.value.get(`${page}:${index}`) ?? 0
  }

  /** Unsaved edits drawn by the renderer, at the size of key `index`, with the values of `page`
   * in its text. */
  function previewUrl(index: number, appearance: Appearance, page?: string) {
    const json = encodeURIComponent(JSON.stringify(appearance))
    return `${IMAGES}/preview/${index}?appearance=${json}${page ? `&page=${page}` : ''}`
  }

  /** The dock showing a screensaver scene, `width` pixels wide. */
  function screensaverUrl(scene: Scene, width: number) {
    return `${IMAGES}/screensaver/${scene}?width=${width}`
  }

  return {
    version: readonly(version),
    name: readonly(name),
    snapshot: readonly(snapshot),
    status: readonly(status),
    pressed: readonly(pressed),
    keyImageUrl,
    redraws,
    previewUrl,
    screensaverUrl,
    assetUrl,
    assetBlob,
    upload,
    refresh,
  }
}
