import { open } from '@tauri-apps/plugin-dialog'
import { commands } from '~/ipc/bindings'
import { pluginEntries, pluginSections } from '~/lib/actions/catalog'
import { catalogOf } from '~/lib/actions/plugins'
import { t } from '~/lib/i18n'
import type { PluginCatalog } from '~/types/engine'

/** Whether they run changes on its own: checked again every few seconds while the app is in view. */
const POLL_MS = 5000

const catalog = ref<PluginCatalog | null>(null)
let started = false

async function refresh() {
  try {
    const fresh = await call(commands.pluginCatalog())
    catalog.value = fresh
    const { sections, entries } = catalogOf(fresh)
    pluginSections.value = sections
    pluginEntries.value = entries
  }
  catch {
    catalog.value = null
  }
}

/** The plugins installed in the app, and what can be done with them. */
export function usePlugins() {
  if (!started && import.meta.client) {
    started = true
    refresh()
    setInterval(() => {
      if (document.visibilityState === 'visible') {
        refresh()
      }
    }, POLL_MS)
  }

  /** A ZIP from a store, or the `.ulanziPlugin` folder itself. Returns the id, or null if the
   * user gave up. */
  async function install(kind: 'zip' | 'folder') {
    const path = await open(kind === 'zip'
      ? { filters: [{ name: t('plugins.list.zipFilter'), extensions: ['zip'] }] }
      : { directory: true })
    if (typeof path !== 'string') {
      return null
    }
    const id = await call(commands.installPlugin(path))
    await refresh()
    return id
  }

  async function uninstall(id: string) {
    await call(commands.uninstallPlugin(id))
    await refresh()
  }

  async function restart(id: string) {
    await call(commands.restartPlugin(id))
    await refresh()
  }

  async function reload() {
    await call(commands.reloadPlugins())
    await refresh()
  }

  async function showFolder() {
    await call(commands.showPluginsFolder())
  }

  return { catalog: readonly(catalog), refresh, install, uninstall, restart, reload, showFolder }
}
