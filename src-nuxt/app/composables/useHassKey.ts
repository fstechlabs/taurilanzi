import type { MaybeRefOrGetter, Ref } from 'vue'
import type { ImageDraft, KeyDraft, StateDraft } from '~/composables/useKeyEditor'
import { followInKey, followedState, offState } from '~/lib/actions/hass'
import { iconSvg } from '~/lib/iconify'
import { isDefaultKeyText } from '~/lib/new-key'
import { newWidget } from '~/lib/widgets'
import type { HassCommand, Widget } from '~/types/engine'

type Card = Extract<Widget, { kind: 'hass_entity' }>

/** How a key looks after an entity of Home Assistant: as a card of the entity, with its icon,
 * name and state always up to date, or with an image and a text of its own. */
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

  /** The card of `entity` on a look, if it has one. */
  function cardOf(look: StateDraft, entity?: string): Card | null {
    const widget = look.widget
    return widget?.kind === 'hass_entity' && (!entity || widget.entity === entity) ? widget : null
  }

  const card = computed(() => (draft?.value ? cardOf(draft.value) : null))

  /** Every look shows the entity as a card, keeping the name and icon chosen for it. */
  async function showCard(entity: string) {
    const key = draft?.value
    if (!key) {
      return
    }
    const byState = await icons(entity)
    for (const look of looks(key)) {
      const previous = cardOf(look)
      const widget = newWidget('hass_entity', look.widget) as Card
      look.widget = {
        ...widget,
        server: toValue(server),
        entity,
        label: previous?.label ?? '',
        icon: previous?.icon ?? null,
        icons: byState,
      }
      if (look.text && isDefaultKeyText(look.text.content)) {
        look.text = null
      }
      if (look.image && !previous) {
        look.image = null
      }
    }
  }

  function hideCard() {
    const key = draft?.value
    if (!key) {
      return
    }
    for (const look of looks(key)) {
      const widget = cardOf(look)
      if (widget) {
        look.widget = null
        look.text = { content: widget.label || hass.entity(widget.entity)?.name || '', fontFamily: 'Inter', fontWeight: 700, size: 20, color: '#ffffff', position: 'bottom' }
      }
    }
  }

  /** A key just set up for an entity, still as it was created, shows the entity as a card; a
   * card of another entity follows the new one. */
  async function dress(entity: string) {
    const key = draft?.value
    if (!key || !hass.entity(entity)) {
      return
    }
    const fresh = !key.image && (!key.text || isDefaultKeyText(key.text.content)) && !key.widget
    if (fresh || (card.value && card.value.entity !== entity)) {
      try {
        await showCard(entity)
      }
      catch {
        // Without the icons the key keeps its look; the card can be turned on by hand.
      }
    }
  }

  /** An icon of the user's on the card, over Home Assistant's; null goes back to them. */
  async function setCardIcon(icon: string | null) {
    const key = draft?.value
    if (!key) {
      return
    }
    const asset = icon ? await uploadIcon(icon) : null
    for (const look of looks(key)) {
      const widget = cardOf(look)
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
      const widget = cardOf(look)
      if (widget) {
        widget.label = label
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

  return { icons, uploadIcon, card, dress, showCard, hideCard, setCardIcon, setCardLabel, setImage, setText, follow, unfollow }
}
