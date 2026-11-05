import { expect, test } from 'bun:test'
import type { CatalogPlugin } from '../../src-nuxt/app/ipc/bindings'
import { facetCounts, filtered, filterPlugins, fold, kindsOf, popularity, sourcesOf } from '../../src-nuxt/app/lib/store-filters'
import type { Labels, StoreFilters, StoreSort } from '../../src-nuxt/app/lib/store-filters'

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
  plugin({ id: 'ulanzi:1', source: 'ulanzi', name: 'OBS Studio', kind: 'streaming', downloads: 5000, installed: '1.0.0', update: true, published_at: '2026-01-01' }),
  plugin({ id: 'ulanzi:2', source: 'ulanzi', name: 'Timer', kind: 'tools', downloads: 4000, published_at: '2026-06-01' }),
  plugin({ id: 'clock.ulanziPlugin', name: 'Clock', author: 'Ana', kind: 'tools', downloads: 30, installed: '2.0.0', tags: ['hora'] }),
  plugin({ id: 'other.ulanziPlugin', name: 'Another', downloads: 20, description: 'Música y más' }),
  plugin({ id: 'mirabox:1', source: 'mirabox', name: 'Spotify', kind: 'audio', downloads: 900_000 }),
  plugin({ id: 'mirabox:2', source: 'mirabox', name: 'Obs timer', kind: 'streaming', downloads: 100 }),
]

const none: StoreFilters = { query: '', source: 'all', status: 'all', kind: 'all' }

function ids(filters: Partial<StoreFilters>, sort: StoreSort = 'popular', labels?: Labels) {
  return filterPlugins(plugins, { ...none, ...filters }, sort, labels).map(p => p.id)
}

test('the most popular of each store come first, however each counts its downloads', () => {
  expect(ids({}).slice(0, 3)).toEqual(['clock.ulanziPlugin', 'ulanzi:1', 'mirabox:1'])
  const scores = popularity(plugins)
  expect(scores.get('mirabox:1')).toBe(1)
  expect(scores.get('mirabox:2')).toBe(0.5)
  expect(scores.get('other.ulanziPlugin')).toBe(0.5)
})

test('sorts by name and by date, the undated last', () => {
  expect(ids({ source: 'ulanzi' }, 'name')).toEqual(['ulanzi:1', 'ulanzi:2'])
  expect(ids({}, 'recent').slice(0, 2)).toEqual(['ulanzi:2', 'ulanzi:1'])
})

test('every filter narrows the list, and plugins of no category are others', () => {
  expect(ids({ source: 'ulanzi' })).toEqual(['ulanzi:1', 'ulanzi:2'])
  expect(ids({ kind: 'tools' })).toEqual(['clock.ulanziPlugin', 'ulanzi:2'])
  expect(ids({ kind: 'other' })).toEqual(['other.ulanziPlugin'])
  expect(ids({ status: 'installed' })).toEqual(['clock.ulanziPlugin', 'ulanzi:1'])
  expect(ids({ status: 'updates' })).toEqual(['ulanzi:1'])
  expect(ids({ source: 'community', kind: 'tools' })).toEqual(['clock.ulanziPlugin'])
})

test('the search looks for every word, without accents, also in the names the app gives', () => {
  expect(ids({ query: 'ana' })).toEqual(['clock.ulanziPlugin'])
  expect(ids({ query: 'HORA clock' })).toEqual(['clock.ulanziPlugin'])
  expect(ids({ query: 'musica' })).toEqual(['other.ulanziPlugin'])
  expect(ids({ query: 'clock obs' })).toEqual([])
  const labels: Labels = p => [p.source === 'mirabox' ? 'Mirabox Space' : '']
  expect(ids({ query: 'space' }, 'popular', labels)).toEqual(['mirabox:1', 'mirabox:2'])
})

test('a search puts first what has the words in its name', () => {
  expect(ids({ query: 'obs' }, 'relevance')).toEqual(['ulanzi:1', 'mirabox:2'])
  expect(ids({ query: 'timer' }, 'relevance')).toEqual(['ulanzi:2', 'mirabox:2'])
  expect(ids({ query: 'obs timer' }, 'relevance')).toEqual(['mirabox:2'])
})

test('counts what each choice would show with the other filters as they are', () => {
  const { sources, statuses, kinds } = facetCounts(plugins, { ...none, kind: 'tools' })
  expect(Object.fromEntries(sources)).toEqual({ all: 2, ulanzi: 1, community: 1 })
  expect(Object.fromEntries(statuses)).toEqual({ all: 2, installed: 1, updates: 0, available: 1 })
  expect(Object.fromEntries(kinds)).toEqual({ all: 6, streaming: 2, tools: 2, other: 1, audio: 1 })
})

test('lists the stores and categories the catalog has, known ones first', () => {
  expect(sourcesOf(plugins)).toEqual(['ulanzi', 'community', 'mirabox'])
  expect(sourcesOf([plugins[2]!])).toEqual(['community'])
  expect(kindsOf(plugins)).toEqual(['tools', 'streaming', 'audio', 'other'])
  expect(filtered(none)).toBe(false)
  expect(filtered({ ...none, query: 'x' })).toBe(false)
  expect(filtered({ ...none, kind: 'audio' })).toBe(true)
  expect(fold('Canción ÑU')).toBe('cancion nu')
})
