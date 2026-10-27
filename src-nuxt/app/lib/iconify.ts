import { shallowReactive } from 'vue'

const API = 'https://api.iconify.design'
/** The public API returns at most 128 results and has no paging for searches. */
const SEARCH_LIMIT = 128
const BROWSE_PAGE = 128
const SVG_CACHE = 5000
/** Iconify rate-limits bursts, so collections are fetched a few at a time. */
const PARALLEL_COLLECTIONS = 4
const THUMBNAIL_COLOR = '#ffffff'

export type IconStyle = 'fill' | 'stroke'
export type IconPalette = 'mono' | 'color'

export interface IconFilters { category?: string, style?: IconStyle, palette?: IconPalette }

export interface IconSearchResult {
  icons: string[]
  /** Collection names for the prefixes present in `icons`. */
  collections: Record<string, string>
  /** More icons match than the service returns. */
  truncated: boolean
}

export interface IconLicense { title: string, spdx?: string, url?: string }

export interface IconCollection {
  prefix: string
  name: string
  total: number
  category: string | null
  /** Icons with their own colours, which a tint does not change. */
  palette: boolean
  author: string | null
  license: IconLicense | null
  samples: string[]
}

export interface IconBrowsePage {
  icons: string[]
  total: number
  page: number
  pages: number
  /** The collection's own categories with how many icons each has. */
  categories: Record<string, number>
}

interface RawCollection {
  name: string
  total?: number
  category?: string
  palette?: boolean
  author?: { name: string }
  license?: IconLicense
  samples?: string[]
}

interface RawIconSet {
  icons?: Record<string, { body: string, width?: number, height?: number, left?: number, top?: number }>
  aliases?: Record<string, { parent: string }>
  width?: number
  height?: number
}

interface Listing { icons: string[], categories: Record<string, string[]> }

const svgs = shallowReactive(new Map<string, string>())
const listings = new Map<string, Listing>()
let collections: Promise<IconCollection[]> | null = null

/** Iconify prefixes and names: lowercase letters, digits and single dashes. */
export function isSlug(text: string) {
  return /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(text) && text.length <= 64
}

export function parseIconId(text: string) {
  const [prefix, name, ...rest] = text.trim().split(':')
  return prefix && name && !rest.length && isSlug(prefix) && isSlug(name) ? `${prefix}:${name}` : null
}

/**
 * Reads the search box: an icones.js.org link to one icon, something shaped like a key
 * (`mdi:hom`, searched within that collection with the exact icon first) or free text,
 * optionally limited to one collection with `prefix:text`.
 */
export async function searchIcons(input: string, prefix: string | null, filters: IconFilters): Promise<IconSearchResult> {
  const text = input.trim()
  const linked = iconFromUrl(text)
  let result: IconSearchResult
  if (linked) {
    result = { icons: [linked], collections: {}, truncated: false }
  }
  else if (parseIconId(text)) {
    const [p, name] = text.split(':') as [string, string]
    result = await search(name, p, {})
    if (!result.icons.includes(text)) {
      result.icons.unshift(text)
    }
  }
  else {
    const [head, ...tail] = text.split(':')
    const scoped = tail.length && head && isSlug(head)
    result = await search(scoped ? tail.join(':').trim() : text, scoped ? head! : prefix, filters)
  }
  await preload(result.icons)
  return { ...result, icons: result.icons.filter(icon => svgs.has(icon)) }
}

export function listCollections() {
  collections ??= get<Record<string, RawCollection>>('/collections')
    .then(raw => Object.entries(raw).map(([prefix, c]) => ({
      prefix,
      name: c.name,
      total: c.total ?? 0,
      category: c.category ?? null,
      palette: c.palette ?? false,
      author: c.author?.name ?? null,
      license: c.license ?? null,
      samples: (c.samples ?? []).slice(0, 3).map(name => `${prefix}:${name}`),
    })))
    .catch((err) => {
      collections = null
      throw err
    })
  return collections
}

/** One page of a collection, optionally limited to one of its categories. */
export async function browseCollection(prefix: string, category: string | null, page: number): Promise<IconBrowsePage> {
  const listing = await loadListing(prefix)
  const names = category ? (listing.categories[category] ?? []) : listing.icons
  const pages = Math.max(1, Math.ceil(names.length / BROWSE_PAGE))
  const current = Math.min(page, pages - 1)
  const icons = names.slice(current * BROWSE_PAGE, (current + 1) * BROWSE_PAGE).map(name => `${prefix}:${name}`)
  await preload(icons)
  return {
    icons,
    total: names.length,
    page: current,
    pages,
    categories: Object.fromEntries(Object.entries(listing.categories).map(([c, list]) => [c, list.length])),
  }
}

/** SVG painted with `currentColor`, so the key can tint it. */
export async function iconSvg(icon: string) {
  if (!svgs.has(icon)) {
    const [prefix, name] = icon.split(':') as [string, string]
    await fetchBodies(prefix, [name])
  }
  const svg = svgs.get(icon)
  if (!svg) {
    throw new Error(`icon not found: ${icon}`)
  }
  return svg
}

/** White version for the dark picker grid, or null while the icon is not loaded. */
export function thumbnail(icon: string) {
  const svg = svgs.get(icon)
  return svg ? `data:image/svg+xml;utf8,${encodeURIComponent(svg.replaceAll('currentColor', THUMBNAIL_COLOR))}` : null
}

/** Fetches the bodies of every uncached icon, one request per collection. */
export async function preload(icons: string[]) {
  const missing = new Map<string, string[]>()
  for (const icon of icons) {
    const id = parseIconId(icon)
    if (id && !svgs.has(id)) {
      const [prefix, name] = id.split(':') as [string, string]
      missing.set(prefix, [...(missing.get(prefix) ?? []), name])
    }
  }
  const queue = [...missing]
  const worker = async () => {
    for (let next = queue.shift(); next; next = queue.shift()) {
      await fetchBodies(...next).catch(() => {})
    }
  }
  await Promise.all(Array.from({ length: PARALLEL_COLLECTIONS }, worker))
}

async function search(text: string, prefix: string | null, filters: IconFilters): Promise<IconSearchResult> {
  const keywords = [
    filters.style && `style=${filters.style}`,
    filters.palette && `palette=${filters.palette === 'color'}`,
  ].filter(Boolean)
  const raw = await get<{ icons: string[], collections?: Record<string, RawCollection> }>('/search', {
    query: [text, ...keywords].join(' '),
    limit: SEARCH_LIMIT,
    prefix: prefix ?? undefined,
    category: filters.category,
  })
  return {
    icons: raw.icons,
    collections: Object.fromEntries(Object.entries(raw.collections ?? {}).map(([p, c]) => [p, c.name])),
    truncated: raw.icons.length >= SEARCH_LIMIT,
  }
}

async function loadListing(prefix: string): Promise<Listing> {
  const cached = listings.get(prefix)
  if (cached) {
    return cached
  }
  const raw = await get<{ uncategorized?: string[], categories?: Record<string, string[]> }>('/collection', { prefix })
  const categories = raw.categories ?? {}
  const icons = [...new Set([...Object.values(categories).flat(), ...(raw.uncategorized ?? [])])].sort()
  const listing = { icons, categories }
  listings.set(prefix, listing)
  return listing
}

async function fetchBodies(prefix: string, names: string[]) {
  const set = await get<RawIconSet>(`/${prefix}.json`, { icons: names.join(',') })
  if (svgs.size + names.length > SVG_CACHE) {
    svgs.clear()
  }
  for (const name of names) {
    const svg = buildSvg(set, name)
    if (svg) {
      svgs.set(`${prefix}:${name}`, svg)
    }
  }
}

function buildSvg(set: RawIconSet, name: string) {
  const icon = set.icons?.[name] ?? set.icons?.[set.aliases?.[name]?.parent ?? '']
  if (!icon) {
    return null
  }
  const width = icon.width ?? set.width ?? 16
  const height = icon.height ?? set.height ?? 16
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="${icon.left ?? 0} ${icon.top ?? 0} ${width} ${height}">${icon.body}</svg>`
}

/** icones.js.org links carry the selected icon as `icon=prefix:name`. */
function iconFromUrl(text: string) {
  if (!/^https?:\/\//.test(text)) {
    return null
  }
  try {
    const icon = new URL(text).searchParams.get('icon')
    return icon ? parseIconId(icon) : null
  }
  catch {
    return null
  }
}

function get<T>(path: string, query?: Record<string, string | number | undefined>) {
  return $fetch<T>(path, { baseURL: API, query })
}
