import type { InjectionKey, MaybeRefOrGetter, Ref } from 'vue'
import { commands } from '~/ipc/bindings'
import type { HassCatalog, HassEntityInfo } from '~/types/engine'
import { domainOf } from '~/lib/actions/hass'
import { t } from '~/lib/i18n'

/** The catalog is read again when asked for after this long, to see what changed in Home Assistant. */
const FRESH_MS = 60_000
const FALLBACK_ICON = 'mdi:bookmark'

/** What each server offers, read when first needed. */
const catalogs = shallowRef(new Map<string, HassCatalog>())
const loadedAt = new Map<string, number>()
const pending = new Map<string, Promise<void>>()
const loading = ref(new Set<string>())
const failed = ref(new Set<string>())

type IconGroup = { default?: string, state?: Record<string, string>, range?: Record<string, string> }
type Icons = Record<string, Record<string, unknown>>

function group(icons: unknown, ...path: string[]): IconGroup | undefined {
  let node = icons as Record<string, unknown> | undefined
  for (const step of path) {
    node = node?.[step] as Record<string, unknown> | undefined
  }
  return node as IconGroup | undefined
}

/** The icon of a range, such as a battery's, for a number. */
function rangeIcon(range: Record<string, string>, value: number) {
  const steps = Object.keys(range).map(Number).filter(Number.isFinite).sort((a, b) => a - b)
  const step = steps.filter(s => s <= value).pop() ?? steps[0]
  return step === undefined ? undefined : range[String(step)]
}

/** The server the fields inside an editor of Home Assistant talk to. */
export const HASS_SERVER: InjectionKey<Ref<string>> = Symbol('hass server')

/** Home Assistant's entities, areas and actions on `server`, or the one of the editor around, with
 * its texts and icons, for the editor. */
export function useHass(server?: MaybeRefOrGetter<string>) {
  const { snapshot } = useEngine()
  const around = server === undefined ? inject(HASS_SERVER, ref('')) : null
  const id = computed(() => (around ? around.value : toValue(server!)))
  const info = computed(() => snapshot.value?.hass[id.value])
  const connected = computed(() => info.value?.status.state === 'connected')
  const catalog = computed(() => catalogs.value.get(id.value) ?? null)

  function mark(set: Ref<Set<string>>, key: string, on: boolean) {
    const next = new Set(set.value)
    if (on) {
      next.add(key)
    }
    else {
      next.delete(key)
    }
    set.value = next
  }

  async function load(force = false) {
    const key = id.value
    if (!key) {
      return
    }
    const running = pending.get(key)
    if (running) {
      return running
    }
    if (!force && catalogs.value.has(key) && Date.now() - (loadedAt.get(key) ?? 0) < FRESH_MS) {
      return
    }
    mark(loading, key, true)
    const task = (async () => {
      try {
        const read = await call(commands.hassCatalog(key))
        catalogs.value = new Map(catalogs.value).set(key, read)
        loadedAt.set(key, Date.now())
        mark(failed, key, false)
      }
      catch {
        mark(failed, key, true)
      }
      finally {
        mark(loading, key, false)
        pending.delete(key)
      }
    })()
    pending.set(key, task)
    return task
  }

  // Connecting brings what to pick from before the key is even saved.
  function connect() {
    if (!id.value) {
      return
    }
    commands.hassConnect(id.value)
    if (connected.value) {
      load()
    }
  }

  watch(connected, (now) => {
    if (now) {
      load()
    }
  })
  watch(id, () => load())

  const entities = computed(() => catalog.value?.entities ?? [])
  const byId = computed(() => new Map(entities.value.map(e => [e.id, e])))

  function entity(id: string): HassEntityInfo | undefined {
    return byId.value.get(id)
  }

  function text(key: string): string | undefined {
    return catalog.value?.texts[key]
  }

  function domainName(domain: string) {
    return text(`component.${domain}.title`) ?? domain
  }

  /** A state, or the value of an attribute, as Home Assistant shows it. */
  function stateText(id: string, state?: string, attribute?: string | null) {
    const found = entity(id)
    const value = state ?? found?.state ?? ''
    if (!attribute && (value === 'unknown' || value === 'unavailable')) {
      return t(`actions.hass.states.${value}`)
    }
    const domain = domainOf(id)
    const tail = attribute ? `state_attributes.${attribute}.state.${value}` : `state.${value}`
    const keys = [
      found?.platform && found.translation_key ? `component.${found.platform}.entity.${domain}.${found.translation_key}.${tail}` : null,
      found?.attributes.device_class ? `component.${domain}.entity_component.${found.attributes.device_class}.${tail}` : null,
      `component.${domain}.entity_component._.${tail}`,
    ]
    for (const key of keys) {
      const found = key ? text(key) : undefined
      if (found) {
        return found
      }
    }
    return value
  }

  function attributeName(id: string, attribute: string) {
    return text(`component.${domainOf(id)}.entity_component._.state_attributes.${attribute}.name`) ?? attribute.replaceAll('_', ' ')
  }

  function areaName(area: string | null | undefined) {
    return catalog.value?.areas.find(a => a.id === area)?.name ?? null
  }

  /** The icon Home Assistant shows for an entity, in a given state or the one it is in. */
  function entityIcon(id: string, state?: string): string {
    const found = entity(id)
    if (found?.icon) {
      return found.icon
    }
    const value = state ?? found?.state ?? ''
    const icons = (catalog.value?.icons ?? {}) as Icons
    const domain = domainOf(id)
    const own = found?.platform && found.translation_key
      ? group(icons.entity, found.platform, domain, found.translation_key)
      : undefined
    const component = group(icons.entity_component, domain)
    const deviceClass = found?.attributes.device_class
    const general = (deviceClass ? group(component, String(deviceClass)) : undefined) ?? group(component, '_')
    for (const candidate of [own, general]) {
      if (!candidate) {
        continue
      }
      const numeric = Number(value)
      const icon = candidate.state?.[value]
        ?? (candidate.range && Number.isFinite(numeric) ? rangeIcon(candidate.range, numeric) : undefined)
        ?? candidate.default
      if (icon) {
        return icon
      }
    }
    return FALLBACK_ICON
  }

  /** Every icon Home Assistant shows for an entity, by state: `''` for any other state and `#20`
   * for values from 20 up, as a battery's. What the key's card draws as the state changes. */
  function entityIcons(id: string): Record<string, string> {
    const found = entity(id)
    if (found?.icon) {
      return { '': found.icon }
    }
    const icons = (catalog.value?.icons ?? {}) as Icons
    const domain = domainOf(id)
    const own = found?.platform && found.translation_key
      ? group(icons.entity, found.platform, domain, found.translation_key)
      : undefined
    const component = group(icons.entity_component, domain)
    const deviceClass = found?.attributes.device_class
    const general = (deviceClass ? group(component, String(deviceClass)) : undefined) ?? group(component, '_')
    const out: Record<string, string> = { '': FALLBACK_ICON }
    for (const candidate of [general, own]) {
      if (!candidate) {
        continue
      }
      if (candidate.default) {
        out[''] = candidate.default
      }
      Object.assign(out, candidate.state ?? {})
      for (const [from, icon] of Object.entries(candidate.range ?? {})) {
        out[`#${from}`] = icon
      }
    }
    return out
  }

  function serviceName(action: string) {
    const [domain, service] = action.split('.')
    return text(`component.${domain}.services.${service}.name`) ?? service?.replaceAll('_', ' ') ?? action
  }

  function serviceDescription(action: string) {
    const [domain, service] = action.split('.')
    return text(`component.${domain}.services.${service}.description`) ?? ''
  }

  function fieldName(action: string, field: string) {
    const [domain, service] = action.split('.')
    return text(`component.${domain}.services.${service}.fields.${field}.name`) ?? field.replaceAll('_', ' ')
  }

  function fieldDescription(action: string, field: string) {
    const [domain, service] = action.split('.')
    return text(`component.${domain}.services.${service}.fields.${field}.description`) ?? ''
  }

  /** An option of a select field, as Home Assistant words it. */
  function selectorOption(domain: string, translationKey: string | undefined, option: string) {
    if (!translationKey) {
      return option
    }
    return text(`component.${domain}.selector.${translationKey}.options.${option}`) ?? option
  }

  return {
    id,
    info,
    connected,
    catalog,
    loading: computed(() => loading.value.has(id.value)),
    failed: computed(() => failed.value.has(id.value)),
    entities,
    load,
    connect,
    entity,
    domainName,
    stateText,
    attributeName,
    areaName,
    entityIcon,
    entityIcons,
    serviceName,
    serviceDescription,
    fieldName,
    fieldDescription,
    selectorOption,
  }
}
