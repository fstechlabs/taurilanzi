const SHOWN_MS = 4500

export interface Toast {
  id: number
  title: string
  message: string
}

const toasts = ref<Toast[]>([])
let next = 1

/** Short messages in a corner of the window, such as those plugins send. */
export function useToasts() {
  function show(title: string, message: string) {
    const id = next++
    toasts.value = [...toasts.value, { id, title, message }]
    setTimeout(() => dismiss(id), SHOWN_MS)
  }

  function dismiss(id: number) {
    toasts.value = toasts.value.filter(t => t.id !== id)
  }

  return { toasts: readonly(toasts), show, dismiss }
}
