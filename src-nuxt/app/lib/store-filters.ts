import type { CatalogPlugin, CatalogSource, PluginKind } from '~/types/engine'

export type StoreStatus = 'all' | 'installed' | 'updates' | 'available'
export type StoreSort = 'popular' | 'name' | 'recent'

export interface StoreFilters {
  query: string
  source: CatalogSource | 'all'
  status: StoreStatus
  kind: PluginKind | 'all'
}

type Plugin = Readonly<Pick<CatalogPlugin, 'id' | 'source' | 'kind' | 'installed' | 'update' | 'name' | 'author' | 'description' | 'category' | 'downloads' | 'published_at'>> & { readonly tags: readonly string[] }

/** The stores in the order the filters list them; any other comes after them. */
export const SOURCES: CatalogSource[] = ['ulanzi', 'community']
export const STATUSES: StoreStatus[] = ['all', 'installed', 'updates', 'available']
export const KINDS: PluginKind[] = ['tools', 'streaming', 'office', 'creative', 'lighting', 'ai']
export const SORTS: StoreSort[] = ['popular', 'name', 'recent']

function words(query: string) {
  return query.toLowerCase().split(/\s+/).filter(Boolean)
}

function matches(plugin: Plugin, filters: StoreFilters, skip?: keyof StoreFilters) {
  if (skip !== 'source' && filters.source !== 'all' && plugin.source !== filters.source) {
    return false
  }
  if (skip !== 'kind' && filters.kind !== 'all' && plugin.kind !== filters.kind) {
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
  const text = [plugin.name, plugin.author, plugin.description, plugin.category, ...plugin.tags].join(' ').toLowerCase()
  return words(filters.query).every(w => text.includes(w))
}

/** The plugins every filter lets through, in the order asked for. */
export function filterPlugins<T extends Plugin>(plugins: readonly T[], filters: StoreFilters, sort: StoreSort): T[] {
  const kept = plugins.filter(p => matches(p, filters))
  const byName = (a: T, b: T) => a.name.localeCompare(b.name)
  if (sort === 'name') {
    return kept.sort(byName)
  }
  if (sort === 'recent') {
    return kept.sort((a, b) => (b.published_at ?? '').localeCompare(a.published_at ?? '') || byName(a, b))
  }
  return kept.sort((a, b) => b.downloads - a.downloads || byName(a, b))
}

/** How many plugins each value of a filter would show, with the other filters as they are. */
export function facetCounts(plugins: readonly Plugin[], filters: StoreFilters) {
  const sources = new Map<CatalogSource | 'all', number>([['all', 0]])
  const statuses = new Map<StoreStatus, number>(STATUSES.map(s => [s, 0]))
  const kinds = new Map<PluginKind | 'all', number>([['all', 0]])
  for (const plugin of plugins) {
    if (matches(plugin, filters, 'source')) {
      sources.set('all', sources.get('all')! + 1)
      sources.set(plugin.source, (sources.get(plugin.source) ?? 0) + 1)
    }
    if (matches(plugin, filters, 'status')) {
      statuses.set('all', statuses.get('all')! + 1)
      if (plugin.installed) {
        statuses.set('installed', statuses.get('installed')! + 1)
      }
      else {
        statuses.set('available', statuses.get('available')! + 1)
      }
      if (plugin.update) {
        statuses.set('updates', statuses.get('updates')! + 1)
      }
    }
    if (matches(plugin, filters, 'kind')) {
      kinds.set('all', kinds.get('all')! + 1)
      if (plugin.kind) {
        kinds.set(plugin.kind, (kinds.get(plugin.kind) ?? 0) + 1)
      }
    }
  }
  return { sources, statuses, kinds }
}

/** The stores the catalog has, known ones first. */
export function sourcesOf(plugins: readonly Plugin[]): CatalogSource[] {
  const present = new Set(plugins.map(p => p.source))
  return [...SOURCES.filter(s => present.has(s)), ...[...present].filter(s => !SOURCES.includes(s)).sort()]
}
