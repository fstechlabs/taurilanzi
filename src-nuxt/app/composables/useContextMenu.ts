import type { Component } from 'vue'

export interface ContextMenuItem {
  label: string
  icon?: Component
  destructive?: boolean
  disabled?: boolean
  /** Draws a separator above the item. */
  separated?: boolean
  run: () => unknown
}

const menu = shallowRef<{ x: number, y: number, items: ContextMenuItem[] } | null>(null)

/** The app's own right-click menu; the browser's is turned off everywhere. */
export function useContextMenu() {
  function open(event: MouseEvent, items: ContextMenuItem[]) {
    event.preventDefault()
    event.stopPropagation()
    menu.value = items.length ? { x: event.clientX, y: event.clientY, items } : null
  }

  function close() {
    menu.value = null
  }

  return { menu, open, close }
}
