import { useLocalStorage } from '@vueuse/core'
import type { IconCollection } from '~/lib/iconify'
import { listCollections } from '~/lib/iconify'
import { t } from '~/lib/i18n'

const RECENT_LIMIT = 24

export const ANY = '*'

export interface PickedIcon { icon: string, multicolor: boolean }

/** Iconify's categories, in the order they are offered, with the key of their name in the messages. */
const CATEGORY_KEYS: Record<string, string> = {
  'UI 24px': 'ui24',
  'UI 16px / 32px': 'ui16And32',
  'UI Other / Mixed Grid': 'uiOther',
  'UI Multicolor': 'uiMulticolor',
  'Material': 'material',
  'Logos': 'logos',
  'Emoji': 'emoji',
  'Flags / Maps': 'flags',
  'Programming': 'programming',
  'Thematic': 'thematic',
  'Archive / Unmaintained': 'archived',
}

const ARCHIVED = 'Archive / Unmaintained'

const collections = ref<IconCollection[]>([])
const status = ref<'idle' | 'loading' | 'error'>('idle')
const recent = useLocalStorage<string[]>('taurilanzi:recent-icons', [])

export function categoryLabel(category: string | null) {
  if (!category) {
    return t('icons.categories.other')
  }
  const key = CATEGORY_KEYS[category]
  return key ? t(`icons.categories.${key}`) : category
}

export function iconesCollectionUrl(prefix: string) {
  return `https://icones.js.org/collection/${prefix}`
}

export function iconesUrl(icon: string) {
  return `${iconesCollectionUrl(icon.split(':')[0] ?? '')}?icon=${icon}`
}

/** Iconify collections, loaded once per session, and the icons picked lately. */
export function useIconCatalog() {
  const byPrefix = computed(() => new Map(collections.value.map(c => [c.prefix, c])))

  /** Collection categories in a stable order, archived ones last. */
  const categories = computed(() => {
    const names = new Set(collections.value.map(c => c.category ?? ''))
    const order = Object.keys(CATEGORY_KEYS)
    return [...names].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))

    function rank(name: string) {
      const index = order.indexOf(name)
      return name === ARCHIVED ? Infinity : index === -1 ? order.length : index
    }
  })

  async function load() {
    if (collections.value.length || status.value === 'loading') {
      return
    }
    status.value = 'loading'
    try {
      collections.value = [...await listCollections()].sort((a, b) => a.name.localeCompare(b.name))
      status.value = 'idle'
    }
    catch {
      status.value = 'error'
    }
  }

  function remember(icon: string) {
    recent.value = [icon, ...recent.value.filter(i => i !== icon)].slice(0, RECENT_LIMIT)
  }

  function isMulticolor(icon: string) {
    return byPrefix.value.get(icon.split(':')[0] ?? '')?.palette ?? false
  }

  return { collections, categories, byPrefix, status, recent, load, remember, isMulticolor }
}
