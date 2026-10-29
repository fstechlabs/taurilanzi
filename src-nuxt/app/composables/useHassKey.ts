import type { MaybeRefOrGetter, Ref } from 'vue'
import type { ImageDraft, KeyDraft, StateDraft } from '~/composables/useKeyEditor'
import { followInKey, followedState, offState, suggestedExtra } from '~/lib/actions/hass'
import { iconSvg } from '~/lib/iconify'
import { isDefaultKeyText } from '~/lib/new-key'
import { newWidget } from '~/lib/widgets'
import type { HassCommand, Widget } from '~/types/engine'

type Card = Extract<Widget, { kind: 'hass_entity' }>
export type HassTile = Extract<Widget, { kind: 'hass_tile' }>
type Live = Card | HassTile

/** How a key shows an entity: drawn as what it is, as a card of its values, or with an image
 * and a text of its own. */
export type HassLook = 'tile' | 'card' | 'image'

/** How a key looks after an entity of Home Assistant: as a tile or a card of the entity, with its
 * icon, name and state always up to date, or with an image and a text of its own. */
export function useHassKey(draft: Ref<KeyDraft | null> | null, server: MaybeRefOrGetter<string>) {
  const hass = useHass(server)
  const { upload } = useEngine()

  async function uploadIcon(icon: string) {
    return await upload(await iconSvg(icon), 'image/svg+xml')
  }

  /** Home Assistant's icons for the entity, uploaded, by state. */
  async function icons(entity: string): Promise<Record<string, string>> {
    const byState = hass.entityIcons(entity)
    const uploaded = new Map<string, string>()
    for (const icon of new Set(Object.values(byState))) {
      uploaded.set(icon, await uploadIcon(icon))
    }
    return Object.fromEntries(Object.entries(byState).map(([state, icon]) => [state, uploaded.get(icon)!]))
  }

  function looks(key: KeyDraft): StateDraft[] {
    return key.second ? [key, key.second] : [key]
  }

  /** The tile or card of `entity` on a look, if it has one. */
  function liveOf(look: StateDraft, entity?: string): Live | null {
    const widget = look.widget
    return (widget?.kind === 'hass_entity' || widget?.kind === 'hass_tile') && (!entity || widget.entity === entity) ? widget : null
  }

  const live = computed(() => (draft?.value ? liveOf(draft.value) : null))
  const card = computed(() => (live.value?.kind === 'hass_entity' ? live.value : null))
  const tile = computed(() => (live.value?.kind === 'hass_tile' ? live.value : null))
  const mode = computed<HassLook>(() => (tile.value ? 'tile' : card.value ? 'card' : 'image'))

  /** Every look shows the entity as a tile or a card, keeping the name and icon chosen for it. */
  async function showLive(entity: string, kind: 'tile' | 'card') {
    const key = draft?.value
    if (!key) {
      return
    }
    const byState = await icons(entity)
    const extra = kind === 'tile' ? suggestedExtra(entity, hass.entities.value) : null
    for (const look of looks(key)) {
      const previous = liveOf(look)
      const common = {
        server: toValue(server),
        entity,
        label: previous?.label ?? '',
        icon: previous?.icon ?? null,
        icons: byState,
      }
      if (kind === 'tile') {
        const kept = previous?.kind === 'hass_tile' && previous.entity === entity ? previous : null
        const widget = newWidget('hass_tile', look.widget) as HassTile
        look.widget = { ...widget, ...kept, ...common, extra: kept ? kept.extra : extra }
      }
      else {
        const widget = newWidget('hass_entity', look.widget) as Card
        look.widget = { ...widget, ...common }
      }
      if (look.text && isDefaultKeyText(look.text.content)) {
        look.text = null
      }
      if (look.image && !previous) {
        look.image = null
      }
    }
  }

  async function showCard(entity: string) {
    await showLive(entity, 'card')
  }

  async function showTile(entity: string) {
    await showLive(entity, 'tile')
  }

  function hideCard() {
    const key = draft?.value
    if (!key) {
      return
    }
    for (const look of looks(key)) {
      const widget = liveOf(look)
      if (widget) {
        look.widget = null
        look.text = { content: widget.label || hass.entity(widget.entity)?.name || '', fontFamily: 'Inter', fontWeight: 700, size: 20, color: '#ffffff', position: 'bottom' }
      }
    }
  }

  async function setMode(entity: string, next: HassLook) {
    if (next === 'image') {
      hideCard()
    }
    else {
      await showLive(entity, next)
    }
  }

  /** A key just set up for an entity, still as it was created, shows the entity as `kind`; a
   * tile or a card of another entity follows the new one. */
  async function dress(entity: string, kind: 'tile' | 'card' = 'card') {
    const key = draft?.value
    if (!key || !hass.entity(entity)) {
      return
    }
    const fresh = !key.image && (!key.text || isDefaultKeyText(key.text.content)) && !key.widget
    const shown = live.value
    if (fresh || (shown && shown.entity !== entity)) {
      try {
        await showLive(entity, fresh ? kind : (shown!.kind === 'hass_tile' ? 'tile' : 'card'))
      }
      catch {
        // Without the icons the key keeps its look; the card can be turned on by hand.
      }
    }
  }

  /** An icon of the user's on the tile or card, over Home Assistant's; null goes back to them. */
  async function setCardIcon(icon: string | null) {
    const key = draft?.value
    if (!key) {
      return
    }
    const asset = icon ? await uploadIcon(icon) : null
    for (const look of looks(key)) {
      const widget = liveOf(look)
      if (widget) {
        widget.icon = asset
      }
    }
  }

  function setCardLabel(label: string) {
    const key = draft?.value
    if (!key) {
      return
    }
    for (const look of looks(key)) {
      const widget = liveOf(look)
      if (widget) {
        widget.label = label
      }
    }
  }

  /** Changes the settings of the tile on every look. */
  function setTile(changes: Partial<HassTile>) {
    const key = draft?.value
    if (!key) {
      return
    }
    for (const look of looks(key)) {
      const widget = liveOf(look)
      if (widget?.kind === 'hass_tile') {
        Object.assign(widget, changes)
      }
    }
  }

  async function entityImage(entity: string, state: string, tint: string): Promise<ImageDraft> {
    const icon = hass.entityIcon(entity, state)
    const asset = await uploadIcon(icon)
    return { asset, source: asset, crop: null, fit: 'contain', inset: 0.22, tint, icon }
  }

  /** On a key without a card: an icon of the user's, or the entity's for off and for on. */
  async function setImage(entity: string, command: HassCommand, icon: string | null) {
    const key = draft?.value
    if (!key) {
      return
    }
    const followed = key.second ? followedState(command) : null
    const tint = (look: StateDraft) => look.image?.tint ?? look.text?.color ?? '#ffffff'
    for (const [look, state] of [[key, followed ? offState(entity) : ''], [key.second, followed?.state ?? '']] as const) {
      if (!look) {
        continue
      }
      if (icon) {
        const asset = await uploadIcon(icon)
        look.image = { asset, source: asset, crop: null, fit: 'contain', inset: 0.22, tint: tint(look), icon }
      }
      else {
        look.image = await entityImage(entity, state, tint(look))
      }
      if (look.text?.position === 'middle') {
        look.text.position = 'bottom'
      }
    }
  }

  function setText(text: string) {
    const key = draft?.value
    if (!key) {
      return
    }
    for (const look of looks(key)) {
      look.text = look.text
        ? { ...look.text, content: text }
        : { content: text, fontFamily: 'Inter', fontWeight: 700, size: 20, color: '#ffffff', position: 'bottom' }
    }
  }

  /** Gives the key a second state that shows while the command's entity is on. A card keeps
   * showing the state by itself; an image of the entity's becomes its icon for off and for on. */
  async function follow(command: HassCommand) {
    const key = draft?.value
    const followed = followedState(command)
    if (!key || !followed) {
      return
    }
    const dressed = Boolean(key.image?.icon && key.image.icon === hass.entityIcon(followed.entity, ''))
    followInKey(key, command, toValue(server))
    if (dressed && followed.state && hass.entity(followed.entity)) {
      try {
        await setImage(followed.entity, command, null)
      }
      catch {
        // Both looks keep the icon they have.
      }
    }
  }

  function unfollow() {
    if (draft?.value) {
      draft.value.second = null
    }
  }

  return {
    icons,
    uploadIcon,
    card,
    tile,
    live,
    mode,
    dress,
    showCard,
    showTile,
    hideCard,
    setMode,
    setCardIcon,
    setCardLabel,
    setTile,
    setImage,
    setText,
    follow,
    unfollow,
  }
}
