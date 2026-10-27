import { commands } from '~/ipc/bindings'
import type { KeyRef } from '~/composables/useWorkspace'

/** Pixels the pointer must travel before a press on a key becomes a drag. */
const THRESHOLD = 5

interface Drag {
  from: KeyRef
  image: string | null
  wide: boolean
  x: number
  y: number
  over: KeyRef | null
}

const drag = shallowRef<Drag | null>(null)
let pending: { from: KeyRef, image: string | null, wide: boolean, x: number, y: number } | null = null
/** A drag ends with a click on the key it started from; that click must not select it. */
let swallowClick = false

function slotAt(x: number, y: number): KeyRef | null {
  const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-key-slot]')
  if (!el?.dataset.page || el.dataset.index === undefined) {
    return null
  }
  return { page: el.dataset.page, index: Number(el.dataset.index) }
}

/** Moving keys by dragging them onto another place of any page on the canvas. */
export function useKeyDrag() {

  function press(event: PointerEvent, from: KeyRef, image: string | null, wide: boolean) {
    if (event.button !== 0 || !image) {
      return
    }
    pending = { from, image, wide, x: event.clientX, y: event.clientY }
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }

  function move(event: PointerEvent) {
    if (!pending && !drag.value) {
      return
    }
    if (pending && Math.hypot(event.clientX - pending.x, event.clientY - pending.y) < THRESHOLD) {
      return
    }
    const from = drag.value ?? pending!
    pending = null
    drag.value = {
      from: from.from,
      image: from.image,
      wide: from.wide,
      x: event.clientX,
      y: event.clientY,
      over: slotAt(event.clientX, event.clientY),
    }
  }

  async function release() {
    pending = null
    const done = drag.value
    if (!done) {
      return
    }
    drag.value = null
    swallowClick = true
    const to = done.over
    if (to && (to.page !== done.from.page || to.index !== done.from.index)) {
      await call(commands.moveKey(done.from, to))
    }
  }

  /** True once for the click that follows a drag. */
  function consumeClick() {
    const swallow = swallowClick
    swallowClick = false
    return swallow
  }

  function isOver(slot: KeyRef) {
    const over = drag.value?.over
    return Boolean(over && over.page === slot.page && over.index === slot.index)
  }

  function isDragging(slot: KeyRef) {
    const from = drag.value?.from
    return Boolean(from && from.page === slot.page && from.index === slot.index)
  }

  return { drag, press, move, release, consumeClick, isOver, isDragging }
}
