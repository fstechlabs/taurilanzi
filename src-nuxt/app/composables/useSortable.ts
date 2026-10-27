import type { CSSProperties } from 'vue'

/** Pixels the pointer must travel before a press becomes a drag. */
const THRESHOLD = 5
const SHIFT_MS = 150

interface Pending {
  list: string
  index: number
  x: number
  y: number
  container: Element
}

interface Drag {
  list: string
  from: number
  to: number
  /** How far the pointer moved since the press. */
  dx: number
  dy: number
}

/** Reordering lists by dragging their items. Items are the `[data-sort-item]` children of the
 * closest `[data-sort-list]`; `list` tells apart several lists sharing one instance. The item
 * follows the pointer and the others make room for it; the list changes on dropping. */
export function useSortable(commit: (list: string, from: number, to: number) => void) {
  const drag = ref<Drag | null>(null)
  let pending: Pending | null = null
  let middles: number[] = []
  /** Center of the dragged item when the drag started. */
  let center = 0
  /** How far the others move to make room: the dragged item and the gap after it. */
  let room = 0
  /** A drag ends with a click on the item it started from, which must not count as a click. */
  let dragged = false

  function press(event: PointerEvent, index: number, list = '') {
    const container = (event.currentTarget as HTMLElement).closest('[data-sort-list]')
    if (event.button !== 0 || !container) {
      return
    }
    pending = { list, index, x: event.clientX, y: event.clientY, container }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', release)
    window.addEventListener('pointercancel', cancel)
  }

  function begin(start: Pending) {
    const rects = [...start.container.querySelectorAll(':scope > [data-sort-item]')]
      .map(item => item.getBoundingClientRect())
    const own = rects[start.index]
    if (!own) {
      return
    }
    middles = rects.map(rect => rect.top + rect.height / 2)
    center = middles[start.index]!
    const next = rects[start.index + 1] ?? rects[start.index - 1]
    const gap = next ? Math.max(0, next.top > own.top ? next.top - own.bottom : own.top - next.bottom) : 0
    room = own.height + gap
    drag.value = { list: start.list, from: start.index, to: start.index, dx: 0, dy: 0 }
    document.body.style.cursor = 'grabbing'
    window.getSelection()?.removeAllRanges()
  }

  function move(event: PointerEvent) {
    if (pending && !drag.value) {
      if (Math.hypot(event.clientX - pending.x, event.clientY - pending.y) < THRESHOLD) {
        return
      }
      begin(pending)
    }
    const current = drag.value
    if (!current || !pending) {
      return
    }
    event.preventDefault()
    current.dx = event.clientX - pending.x
    current.dy = event.clientY - pending.y
    const at = center + current.dy
    current.to = middles.filter((middle, i) => i !== current.from && middle < at).length
  }

  function stop() {
    pending = null
    document.body.style.cursor = ''
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', release)
    window.removeEventListener('pointercancel', cancel)
  }

  function release() {
    const done = drag.value
    stop()
    drag.value = null
    if (done) {
      dragged = true
      setTimeout(() => {
        dragged = false
      })
      if (done.to !== done.from) {
        commit(done.list, done.from, done.to)
      }
    }
  }

  function cancel() {
    stop()
    drag.value = null
  }

  onBeforeUnmount(stop)

  /** Where an item is drawn while dragging: under the pointer, or out of the dragged one's way. */
  function itemStyle(index: number, list = ''): CSSProperties {
    const current = drag.value
    if (!current || current.list !== list) {
      return {}
    }
    if (index === current.from) {
      return {
        transform: `translate(${current.dx}px, ${current.dy}px) scale(1.02)`,
        position: 'relative',
        zIndex: 20,
        pointerEvents: 'none',
      }
    }
    const { from, to } = current
    const shift = from < to && index > from && index <= to
      ? -room
      : to < from && index >= to && index < from ? room : 0
    return { transform: `translateY(${shift}px)`, transition: `transform ${SHIFT_MS}ms ease` }
  }

  function isDragging(index: number, list = '') {
    return drag.value?.list === list && drag.value.from === index
  }

  /** Whether the click that just happened ended a drag. */
  function wasDrag() {
    return dragged
  }

  return { press, itemStyle, isDragging, wasDrag, active: computed(() => Boolean(drag.value)) }
}

/** Moves an item of an array to another place. */
export function moveItem<T>(list: T[], from: number, to: number) {
  const [item] = list.splice(from, 1)
  list.splice(to, 0, item!)
}
