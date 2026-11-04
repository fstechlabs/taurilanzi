import { expect, test } from 'bun:test'
import type { CatalogPlugin } from '../../src-nuxt/app/ipc/bindings'
import { facetCounts, filterPlugins, sourcesOf } from '../../src-nuxt/app/lib/store-filters'
import type { StoreFilters } from '../../src-nuxt/app/lib/store-filters'

function plugin(changes: Partial<CatalogPlugin>): CatalogPlugin {
  return {
    id: 'x',
    source: 'community',
    plugin: 'com.x',
    name: 'X',
    author: '',
    version: '1.0.0',
    description: '',
    long_description: '',
    category: '',
    kind: null,
    icon: null,
    screenshots: [],
    tags: [],
    changelog: '',
    published_at: null,
    downloads: 0,
    stars: 0,
    source_url: null,
    security: { status: '', critical: 0, high: 0, secrets: 0 },
    dials: false,
    installed: null,
    update: false,
    ...changes,
  }
}

const plugins = [
  plugin({ id: 'ulanzi:1', source: 'ulanzi', name: 'OBS Studio', kind: 'streaming', downloads: 50, installed: '1.0.0', update: true, published_at: '2026-01-01' }),
  plugin({ id: 'ulanzi:2', source: 'ulanzi', name: 'Timer', kind: 'tools', downloads: 10, published_at: '2026-06-01' }),
  plugin({ id: 'clock.ulanziPlugin', name: 'Clock', author: 'Ana', kind: 'tools', downloads: 30, installed: '2.0.0', tags: ['hora'] }),
  plugin({ id: 'other.ulanziPlugin', name: 'Another', downloads: 30 }),
]

const none: StoreFilters = { query: '', source: 'all', status: 'all', kind: 'all' }

function ids(filters: Partial<StoreFilters>, sort: 'popular' | 'name' | 'recent' = 'popular') {
  return filterPlugins(plugins, { ...none, ...filters }, sort).map(p => p.id)
}

test('sorts by downloads, then by name', () => {
  expect(ids({})).toEqual(['ulanzi:1', 'other.ulanziPlugin', 'clock.ulanziPlugin', 'ulanzi:2'])
  expect(ids({}, 'name')).toEqual(['other.ulanziPlugin', 'clock.ulanziPlugin', 'ulanzi:1', 'ulanzi:2'])
  expect(ids({}, 'recent')).toEqual(['ulanzi:2', 'ulanzi:1', 'other.ulanziPlugin', 'clock.ulanziPlugin'])
})

test('every filter narrows the list', () => {
  expect(ids({ source: 'ulanzi' })).toEqual(['ulanzi:1', 'ulanzi:2'])
  expect(ids({ kind: 'tools' })).toEqual(['clock.ulanziPlugin', 'ulanzi:2'])
  expect(ids({ status: 'installed' })).toEqual(['ulanzi:1', 'clock.ulanziPlugin'])
  expect(ids({ status: 'updates' })).toEqual(['ulanzi:1'])
  expect(ids({ status: 'available' })).toEqual(['other.ulanziPlugin', 'ulanzi:2'])
  expect(ids({ source: 'community', kind: 'tools' })).toEqual(['clock.ulanziPlugin'])
})

test('the search looks for every word in names, authors and tags', () => {
  expect(ids({ query: 'ana' })).toEqual(['clock.ulanziPlugin'])
  expect(ids({ query: 'HORA clock' })).toEqual(['clock.ulanziPlugin'])
  expect(ids({ query: 'clock obs' })).toEqual([])
})

test('counts what each choice would show with the other filters as they are', () => {
  const { sources, statuses, kinds } = facetCounts(plugins, { ...none, kind: 'tools' })
  expect(Object.fromEntries(sources)).toEqual({ all: 2, ulanzi: 1, community: 1 })
  expect(Object.fromEntries(statuses)).toEqual({ all: 2, installed: 1, updates: 0, available: 1 })
  expect(Object.fromEntries(kinds)).toEqual({ all: 4, streaming: 1, tools: 2 })
})

test('lists the known stores first', () => {
  expect(sourcesOf(plugins)).toEqual(['ulanzi', 'community'])
  expect(sourcesOf([plugins[2]!])).toEqual(['community'])
})
