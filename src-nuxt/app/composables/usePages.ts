import type { DeepReadonly } from 'vue'
import { commands } from '~/ipc/bindings'
import type { Action, Key, Page, Position } from '~/types/engine'
import type { NewKeyStyle } from '~/lib/new-key'
import { newKeyAppearance } from '~/lib/new-key'
import { t } from '~/lib/i18n'

const NAVIGATION = new Set<Action['type']>(['open_page', 'back', 'home', 'previous_page', 'next_page', 'switch_profile'])

export function isNavigation(action: Action) {
  return NAVIGATION.has(action.type)
}

/** The line drawn on the canvas replaces the key's navigation, or goes first if it had none. */
export function withLink(actions: Action[], target: string): Action[] {
  const link: Action = { type: 'open_page', page: target }
  const at = actions.findIndex(isNavigation)
  return at === -1 ? [link, ...actions] : actions.map((a, i) => (i === at ? link : a))
}

/** Page and line edits from the canvas, saved straight away. */
export function usePages() {
  const { snapshot } = useEngine()

  function page(id: string): DeepReadonly<Page> | undefined {
    return snapshot.value?.pages.find(p => p.id === id)
  }

  function newKeyStyle() {
    return snapshot.value!.preferences.new_keys as NewKeyStyle
  }

  async function create(position: Position, name?: string) {
    return await call(commands.createPage(name ?? null, position))
  }

  async function update(id: string, changes: { name?: string, position?: Position }) {
    await call(commands.updatePage(id, { name: changes.name ?? null, position: changes.position ?? null }))
  }

  async function duplicate(id: string) {
    return await call(commands.duplicatePage(id))
  }

  async function remove(id: string) {
    await call(commands.deletePage(id))
  }

  async function setHome(id: string) {
    await call(commands.setHomePage(id))
  }

  async function show(id: string) {
    await call(commands.showPage(id))
  }

  async function saveKey(pageId: string, index: number, key: Key) {
    await call(commands.setKey(pageId, index, key))
  }

  /** An empty key gets the look of new keys with the target's name, so the new line is visible. */
  async function link(pageId: string, index: number, target: string) {
    const existing = page(pageId)?.keys[index]
    const key: Key = existing
      ? { ...(structuredClone(toRaw(existing)) as Key), actions: withLink(existing.actions as Action[], target) }
      : {
          appearance: newKeyAppearance(newKeyStyle(), page(target)?.name ?? t('editor.newKey.page')),
          actions: [{ type: 'open_page', page: target }],
          double_press: [],
          long_press: [],
          second_state: null,
        }
    await saveKey(pageId, index, key)
  }

  /** Deleting a key takes its line with it, and the other way round. */
  async function removeKey(pageId: string, index: number) {
    await call(commands.clearKey(pageId, index))
  }

  /** Saves the points the user placed on the line of a key. */
  async function setWaypoints(pageId: string, index: number, waypoints: Position[]) {
    const existing = page(pageId)?.keys[index]
    if (!existing) {
      return
    }
    const rounded = waypoints.map(p => ({ x: Math.round(p.x), y: Math.round(p.y) }))
    const actions = (existing.actions as Action[]).map(a => (a.type === 'open_page' ? { ...a, waypoints: rounded } : a))
    await saveKey(pageId, index, { ...(structuredClone(toRaw(existing)) as Key), actions })
  }

  return { page, create, update, duplicate, remove, setHome, show, link, removeKey, setWaypoints }
}
