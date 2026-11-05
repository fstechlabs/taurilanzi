import type { CatalogPlugin, CatalogSource, PluginKind } from '~/types/engine'

export type StoreStatus = 'all' | 'installed' | 'updates' | 'available'
/** `relevance` only while searching. */
export type StoreSort = 'relevance' | 'popular' | 'recent' | 'name'
/** The categories of the stores, and `other` for plugins in none of them. */
export type StoreKind = PluginKind | 'other'

export interface StoreFilters {
  query: string
  source: CatalogSource | 'all'
  status: StoreStatus
  kind: StoreKind | 'all'
}

type Plugin = Readonly<Pick<CatalogPlugin, 'id' | 'source' | 'kind' | 'installed' | 'update' | 'name' | 'author' | 'description' | 'category' | 'downloads' | 'published_at'>> & { readonly tags: readonly string[] }

/** The stores in the order the filters list them; any other comes after them. */
export const SOURCES: CatalogSource[] = ['ulanzi', 'community', 'mirabox']
export const STATUSES: StoreStatus[] = ['all', 'installed', 'updates', 'available']
export const KINDS: PluginKind[] = ['tools', 'streaming', 'office', 'creative', 'audio', 'lighting', 'gaming', 'social', 'smart_home', 'ai']
/** Those the list can be sorted by when nothing is searched. */
export const SORTS: StoreSort[] = ['popular', 'recent', 'name']

/** Lower case and without accents, so `musica` finds `Música`. */
export function fold(text: string) {
  return text.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase()
}

function words(query: string) {
  return fold(query).split(/\s+/).filter(Boolean)
}

function kindOf(plugin: Plugin): StoreKind {
  return plugin.kind ?? 'other'
}

/** What a search looks in besides the plugin's own texts: names the app gives it, such as its
 * category and its store in the user's language. */
export type Labels = (plugin: Plugin) => readonly string[]

function fields(plugin: Plugin, labels?: Labels) {
  return {
    name: fold(plugin.name),
    others: fold([plugin.author, plugin.category, ...plugin.tags, ...(labels?.(plugin) ?? [])].join(' ')),
    description: fold(plugin.description),
  }
}

function matches(plugin: Plugin, filters: StoreFilters, labels?: Labels, skip?: keyof StoreFilters) {
  if (skip !== 'source' && filters.source !== 'all' && plugin.source !== filters.source) {
    return false
  }
  if (skip !== 'kind' && filters.kind !== 'all' && kindOf(plugin) !== filters.kind) {
    return false
  }
  if (skip !== 'status') {
    if (filters.status === 'installed' && !plugin.installed) {
      return false
    }
    if (filters.status === 'updates' && !plugin.update) {
      return false
    }
    if (filters.status === 'available' && plugin.installed) {
      return false
    }
  }
  const found = fields(plugin, labels)
  const text = `${found.name} ${found.others} ${found.description}`
  return words(filters.query).every(w => text.includes(w))
}

/** How well a plugin answers a search: in its name first, at the start of a word best. */
function relevance(plugin: Plugin, query: string, labels?: Labels) {
  const found = fields(plugin, labels)
  const whole = fold(query.trim())
  let score = found.name === whole ? 100 : found.name.startsWith(whole) ? 50 : 0
  for (const word of words(query)) {
    if (found.name.split(/[^\p{L}\p{N}]+/u).some(part => part.startsWith(word))) {
      score += 20
    }
    else if (found.name.includes(word)) {
      score += 10
    }
    else if (found.others.includes(word)) {
      score += 4
    }
    else {
      score += 1
    }
  }
  return score
}

/** Each store counts its downloads its own way, a few hundred in one and millions in another:
 * a plugin is as popular as its place in its own store, from 1 for the first towards 0. */
export function popularity(plugins: readonly Plugin[]) {
  const bySource = new Map<string, Plugin[]>()
  for (const plugin of plugins) {
    bySource.set(plugin.source, [...(bySource.get(plugin.source) ?? []), plugin])
  }
  const scores = new Map<string, number>()
  for (const list of bySource.values()) {
    const sorted = [...list].sort((a, b) => b.downloads - a.downloads)
    let place = 0
    sorted.forEach((plugin, i) => {
      if (plugin.downloads !== sorted[place]!.downloads) {
        place = i
      }
      scores.set(plugin.id, 1 - place / sorted.length)
    })
  }
  return scores
}

/** The plugins every filter lets through, in the order asked for. */
export function filterPlugins<T extends Plugin>(plugins: readonly T[], filters: StoreFilters, sort: StoreSort, labels?: Labels): T[] {
  const kept = plugins.filter(p => matches(p, filters, labels))
  const popular = popularity(plugins)
  const byName = (a: T, b: T) => a.name.localeCompare(b.name)
  const byPopularity = (a: T, b: T) => (popular.get(b.id) ?? 0) - (popular.get(a.id) ?? 0) || byName(a, b)
  switch (sort) {
    case 'name':
      return kept.sort(byName)
    case 'recent':
      return kept.sort((a, b) => (b.published_at ?? '').localeCompare(a.published_at ?? '') || byPopularity(a, b))
    case 'relevance': {
      const scores = new Map(kept.map(p => [p.id, relevance(p, filters.query, labels)]))
      return kept.sort((a, b) => scores.get(b.id)! - scores.get(a.id)! || byPopularity(a, b))
    }
    default:
      return kept.sort(byPopularity)
  }
}

/** How many plugins each value of a filter would show, with the other filters as they are. */
export function facetCounts(plugins: readonly Plugin[], filters: StoreFilters, labels?: Labels) {
  const sources = new Map<CatalogSource | 'all', number>([['all', 0]])
  const statuses = new Map<StoreStatus, number>(STATUSES.map(s => [s, 0]))
  const kinds = new Map<StoreKind | 'all', number>([['all', 0]])
  const add = <K>(map: Map<K, number>, key: K) => map.set(key, (map.get(key) ?? 0) + 1)
  for (const plugin of plugins) {
    if (matches(plugin, filters, labels, 'source')) {
      add(sources, 'all')
      add(sources, plugin.source)
    }
    if (matches(plugin, filters, labels, 'status')) {
      add(statuses, 'all')
      add(statuses, plugin.installed ? 'installed' : 'available')
      if (plugin.update) {
        add(statuses, 'updates')
      }
    }
    if (matches(plugin, filters, labels, 'kind')) {
      add(kinds, 'all')
      add(kinds, kindOf(plugin))
    }
  }
  return { sources, statuses, kinds }
}

/** The stores the catalog has, known ones first. */
export function sourcesOf(plugins: readonly Plugin[]): CatalogSource[] {
  const present = new Set(plugins.map(p => p.source))
  return [...SOURCES.filter(s => present.has(s)), ...[...present].filter(s => !SOURCES.includes(s)).sort()]
}

/** The categories some plugin of the catalog is in, `other` last. */
export function kindsOf(plugins: readonly Plugin[]): StoreKind[] {
  const present = new Set(plugins.map(kindOf))
  return [...KINDS.filter(k => present.has(k)), ...(present.has('other') ? ['other' as const] : [])]
}

/** Whether a filter of the sidebar is set; the search has a clear button of its own. */
export function filtered(filters: StoreFilters) {
  return filters.source !== 'all' || filters.status !== 'all' || filters.kind !== 'all'
}
