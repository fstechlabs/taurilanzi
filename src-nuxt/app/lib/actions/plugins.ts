import { Puzzle } from '@lucide/vue'
import type { CatalogEntry, Section } from '~/lib/actions/catalog'
import type { DeepReadonly } from 'vue'
import type { Action, PluginActionInfo, PluginCatalog, PluginInfo } from '~/types/engine'
// Relative, not `~`: the unit tests import this file and bun does not know that alias.
import { t } from '../i18n'

type Catalog = DeepReadonly<PluginCatalog>
type Plugin = DeepReadonly<PluginInfo>
type PluginAction = DeepReadonly<PluginActionInfo>

/** Images of a plugin's package, served by the engine. */
const IMAGES = typeof navigator !== 'undefined' && navigator.userAgent.includes('Windows') ? 'http://deck.localhost' : 'deck://localhost'

function encodePath(path: string) {
  return path.replace(/^\.?\//, '').split('/').map(encodeURIComponent).join('/')
}

export function pluginImage(plugin: string, path: string | null | undefined) {
  return path ? `${IMAGES}/plugin/${encodeURIComponent(plugin)}/${encodePath(path)}` : undefined
}

/** An icon or screenshot of the community catalog, fetched by the engine. */
export function communityImage(url: string | null | undefined) {
  return url ? `${IMAGES}/community?url=${encodeURIComponent(url)}` : undefined
}

/** The id of a plugin is the first four segments of the ids of its actions. */
export function pluginOf(action: string) {
  return action.split('.').slice(0, 4).join('.')
}

/** Key 7 of the D200 is `2_1` for plugins: column and row. */
export function keyName(index: number) {
  return `${index % 5}_${Math.floor(index / 5)}`
}

/** The property inspector page of an action, with the query Ulanzi Studio gives it. */
export function inspectorUrl(catalog: Catalog, plugin: Plugin, action: PluginAction, index: number, actionid: string) {
  if (!action.inspector) {
    return null
  }
  const query = new URLSearchParams({
    address: '127.0.0.1',
    port: String(catalog.port),
    uuid: action.uuid,
    key: keyName(index),
    actionid,
    language: catalog.language,
    controller: 'Keypad',
    device: 'D200',
  })
  return `http://127.0.0.1:${catalog.port}/plugins/${encodeURIComponent(plugin.folder)}/${encodePath(action.inspector)}?${query}`
}

export function findAction(catalog: Catalog | null, uuid: string) {
  const plugin = catalog?.plugins.find(p => p.id === pluginOf(uuid))
  const action = plugin?.actions.find(a => a.uuid === uuid)
  return plugin && action ? { plugin, action } : null
}

export function newPluginAction(uuid: string): Action {
  return { type: 'plugin', action: uuid, id: crypto.randomUUID(), settings: {} }
}

/** One section per plugin, with its actions; those it cannot run here show why. */
export function catalogOf(catalog: Catalog): { sections: Section[], entries: CatalogEntry[] } {
  const sections = catalog.plugins
    .filter(plugin => plugin.actions.length)
    .map((plugin): Section => ({
      id: `plugin:${plugin.id}`,
      name: plugin.category || plugin.name,
      get description() {
        return plugin.author ? t('actions.plugins.sectionBy', { author: plugin.author }) : t('actions.plugins.section')
      },
    }))
  const entries = catalog.plugins.flatMap(plugin => plugin.actions.map((action): CatalogEntry => ({
    id: `plugin:${action.uuid}`,
    section: `plugin:${plugin.id}`,
    name: action.name,
    description: action.tooltip || plugin.name,
    icon: Puzzle,
    image: pluginImage(plugin.id, action.icon ?? plugin.icon),
    keywords: ['plugin', plugin.name, plugin.author],
    plugin: true,
    modal: true,
    get soon() {
      return unavailable(plugin, action)
    },
    create: () => newPluginAction(action.uuid),
    matches: candidate => candidate.type === 'plugin' && candidate.action === action.uuid,
  })))
  return { sections, entries }
}

function unavailable(plugin: Plugin, action: PluginAction) {
  if (!action.supported) {
    return t('actions.plugins.forDials')
  }
  return plugin.run.state === 'unavailable' ? plugin.run.reason : undefined
}
