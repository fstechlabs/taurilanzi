const SHOWN_MS = 4500
/** Long enough to reach for its button. */
const WITH_ACTION_MS = 8000

export interface ToastAction {
  label: string
  run: () => unknown
}

export interface Toast {
  id: number
  title: string
  message: string
  action?: ToastAction
}

export interface ToastOptions {
  action?: ToastAction
  /** A toast with the same key replaces the one shown, so a burst of the same notice shows once. */
  key?: string
}

const toasts = ref<Toast[]>([])
const timers = new Map<number, ReturnType<typeof setTimeout>>()
const keyed = new Map<string, number>()
let next = 1

function dismiss(id: number) {
  clearTimeout(timers.get(id))
  timers.delete(id)
  for (const [key, value] of keyed) {
    if (value === id) {
      keyed.delete(key)
    }
  }
  toasts.value = toasts.value.filter(t => t.id !== id)
}

/** Short messages in a corner of the window, such as those plugins send. */
export function useToasts() {
  function show(title: string, message: string, options: ToastOptions = {}) {
    const shown = options.key === undefined ? undefined : keyed.get(options.key)
    const id = shown ?? next++
    const toast: Toast = { id, title, message, action: options.action }
    toasts.value = shown === undefined
      ? [...toasts.value, toast]
      : toasts.value.map(t => (t.id === id ? toast : t))
    if (options.key !== undefined) {
      keyed.set(options.key, id)
    }
    clearTimeout(timers.get(id))
    timers.set(id, setTimeout(() => dismiss(id), options.action ? WITH_ACTION_MS : SHOWN_MS))
  }

  return { toasts: readonly(toasts), show, dismiss }
}
