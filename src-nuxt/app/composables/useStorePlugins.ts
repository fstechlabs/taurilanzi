import { commands } from '~/ipc/bindings'
import type { CatalogPlugin } from '~/types/engine'

const plugins = ref<CatalogPlugin[] | null>(null)
const loading = ref(false)
const failure = ref<string | null>(null)
/** Ids of the store entries being installed right now. */
const installing = ref<Set<string>>(new Set())

async function load(refresh = false) {
  loading.value = true
  try {
    plugins.value = await call(commands.storePlugins(refresh))
    failure.value = null
  }
  catch (err) {
    failure.value = err instanceof Error ? err.message : String(err)
  }
  finally {
    loading.value = false
  }
}

/** The plugins of the stores that run here, and installing them from the app. */
export function useStorePlugins() {
  const { refresh: refreshInstalled } = usePlugins()

  async function install(id: string) {
    installing.value = new Set([...installing.value, id])
    try {
      const installed = await call(commands.installStorePlugin(id))
      await refreshInstalled()
      await load()
      return installed
    }
    finally {
      const rest = new Set(installing.value)
      rest.delete(id)
      installing.value = rest
    }
  }

  /** The entry of the stores for an installed plugin, if one has it. */
  function entryOf(plugin: string) {
    return plugins.value?.find(p => p.plugin === plugin) ?? null
  }

  return {
    plugins: readonly(plugins),
    loading: readonly(loading),
    failure: readonly(failure),
    installing: readonly(installing),
    load,
    install,
    entryOf,
  }
}
