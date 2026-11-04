import type { KeyDraft, StateDraft } from '~/composables/useKeyEditor'
import type { CatalogEntry, NewActionContext } from '~/lib/actions/catalog'
import { BASIC, allEntries, entryFor } from '~/lib/actions/catalog'
import { isDefaultKeyText } from '~/lib/new-key'
import { newWidget } from '~/lib/widgets'
import type { Action, KeyKind } from '~/types/engine'

export const PAGE_INDICATOR_TEXT = '{page_number}/{page_count}'

export function kindOfEntry(entry: CatalogEntry): KeyKind {
  switch (entry.setup) {
    case 'basic':
      return 'basic'
    case 'widget':
      return 'widget'
    case 'carousel':
      return 'carousel'
    case 'two_states':
    case 'switch_hotkey':
      return 'two_states'
    case 'page_indicator':
      return 'page_indicator'
    default:
      return 'action'
  }
}

/** The entry of the catalog the key is set up as. */
export function typeEntry(draft: KeyDraft): CatalogEntry | undefined {
  switch (draft.kind) {
    case 'action':
      return draft.main ? entryFor(draft.main) : undefined
    case 'widget':
      return allEntries().find(entry => entry.widget && entry.widget === draft.widget?.kind)
    case 'carousel':
      return allEntries().find(entry => entry.id === 'carousel')
    case 'two_states':
      return allEntries().find(entry => entry.id === 'toggle_sequences')
    case 'page_indicator':
      return allEntries().find(entry => entry.id === 'page_indicator')
    default:
      return BASIC
  }
}

/** Whether the type has something to set up in its dialog. */
export function hasTypeSettings(draft: KeyDraft): boolean {
  return draft.kind === 'action' || draft.kind === 'widget' || draft.kind === 'carousel'
}

/** A copy of a look with no actions, for the second state of a key. */
export function copyLook(state: StateDraft): StateDraft {
  const { background, from, to, angle, image, text, untouched, widget, moreWidgets } = state
  return { ...JSON.parse(JSON.stringify({ background, from, to, angle, image, text, untouched, widget, moreWidgets })) as Omit<StateDraft, 'main' | 'actions'>, main: null, actions: [] }
}

function hotkey(): Action {
  return { type: 'hotkey', keys: { key: '', command: false, shift: false, option: false, control: false } }
}

/** Makes the key of the type of `entry`. Its look and its other actions stay; whatever the type
 * it had brought goes. */
export function applyType(draft: KeyDraft, entry: CatalogEntry, context: NewActionContext) {
  const kind = kindOfEntry(entry)
  if (draft.kind === 'page_indicator' && draft.text?.content === PAGE_INDICATOR_TEXT) {
    draft.text = null
  }
  draft.kind = kind
  draft.main = null
  draft.widget = null
  draft.moreWidgets = []
  draft.second = null
  switch (entry.setup) {
    case 'basic':
      return
    case 'widget':
      draft.widget = newWidget(entry.widget!, null)
      if (isDefaultKeyText(draft.text?.content)) {
        draft.text = null
      }
      return
    case 'carousel': {
      const first = newWidget('digital_clock', null)
      draft.widget = first
      draft.moreWidgets = [newWidget('calendar', first)]
      if (isDefaultKeyText(draft.text?.content)) {
        draft.text = null
      }
      return
    }
    case 'page_indicator':
      draft.text = draft.text
        ? { ...draft.text, content: PAGE_INDICATOR_TEXT }
        : { content: PAGE_INDICATOR_TEXT, fontFamily: 'Inter', fontWeight: 700, size: 40, color: '#ffffff', position: 'middle' }
      return
    case 'two_states':
    case 'switch_hotkey':
      draft.second = copyLook(draft)
      if (entry.setup === 'switch_hotkey') {
        draft.actions.unshift(hotkey())
        draft.second.actions.push(hotkey())
      }
      return
    default:
      draft.main = entry.create?.(context) ?? null
      // The plugin draws the key: the text of a new key would only cover it.
      if (entry.plugin && isDefaultKeyText(draft.text?.content)) {
        draft.text = null
      }
  }
}
