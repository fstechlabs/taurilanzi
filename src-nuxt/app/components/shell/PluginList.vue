<script setup lang="ts">
import { ArrowUpCircle, FileArchive, FolderOpen, FolderSearch, Plus, Puzzle, RefreshCw, Store } from '@lucide/vue'
import type { DeepReadonly } from 'vue'
import { pluginImage } from '~/lib/actions/plugins'
import type { PluginInfo } from '~/types/engine'

type Plugin = DeepReadonly<PluginInfo>

const { catalog, install, reload, showFolder } = usePlugins()
const community = useCommunityPlugins()
const { show } = useToasts()
const { t } = useI18n()
const browsing = ref(false)

onMounted(() => {
  if (!community.plugins.value) {
    community.load()
  }
})

const plugins = computed(() => catalog.value?.plugins ?? [])
const open = ref<string | null>(null)
const opened = computed(() => plugins.value.find(p => p.id === open.value) ?? null)
const dialogOpen = computed({
  get: () => Boolean(opened.value),
  set: (value) => {
    if (!value) {
      open.value = null
    }
  },
})

function status(plugin: Plugin): { color: string, label: string } {
  switch (plugin.run.state) {
    case 'running':
      return plugin.connected
        ? { color: 'bg-emerald-500', label: t('plugins.status.running') }
        : { color: 'bg-amber-400', label: t('plugins.status.starting') }
    case 'restarting':
      return { color: 'bg-amber-400', label: t('plugins.status.restarting') }
    case 'unavailable':
      return { color: 'bg-red-500', label: plugin.run.reason }
    case 'passive':
      return { color: 'bg-neutral-500', label: t('plugins.status.passive') }
    default:
      return { color: 'bg-neutral-500', label: t('plugins.status.stopped') }
  }
}

async function add(kind: 'zip' | 'folder') {
  try {
    const id = await install(kind)
    if (id) {
      open.value = id
    }
  }
  catch (err) {
    show(t('plugins.installFailed'), err instanceof Error ? err.message : String(err))
  }
}
</script>

<template>
  <div class="flex flex-col" data-plugin-list>
    <div class="flex items-center px-2 pt-4 pb-1">
      <p class="flex-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        {{ t('plugins.list.title') }}
      </p>
      <DropdownMenu>
        <DropdownMenuTrigger as-child>
          <button type="button" class="rounded p-0.5 text-muted-foreground hover:bg-neutral-800 hover:text-foreground" :aria-label="t('plugins.list.add')">
            <Plus class="size-3.5" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" class="w-60">
          <DropdownMenuItem @select="browsing = true">
            <Store /> {{ t('plugins.list.community') }}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem @select="add('zip')">
            <FileArchive /> {{ t('plugins.list.fromZip') }}
          </DropdownMenuItem>
          <DropdownMenuItem @select="add('folder')">
            <FolderSearch /> {{ t('plugins.list.fromFolder') }}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem @select="reload()">
            <RefreshCw /> {{ t('plugins.list.reload') }}
          </DropdownMenuItem>
          <DropdownMenuItem @select="showFolder()">
            <FolderOpen /> {{ t('plugins.list.openFolder') }}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
    <p v-if="!catalog" class="px-2 py-1 text-xs text-muted-foreground">
      {{ t('plugins.list.failed') }}
    </p>
    <div v-else-if="!plugins.length" class="flex items-center gap-1.5 px-2 py-1 text-xs text-muted-foreground">
      <Puzzle class="size-4" /> {{ t('plugins.list.none') }}
    </div>
    <button
      v-for="plugin in plugins"
      :key="plugin.id"
      type="button"
      class="flex items-center gap-2 rounded px-2 py-1 text-left hover:bg-neutral-800/60"
      :data-plugin="plugin.id"
      @click="open = plugin.id"
    >
      <img v-if="plugin.icon" :src="pluginImage(plugin.id, plugin.icon)" alt="" class="size-4 shrink-0 rounded-sm">
      <Puzzle v-else class="size-4 shrink-0 text-muted-foreground" />
      <span class="min-w-0 flex-1 truncate">{{ plugin.name }}</span>
      <ArrowUpCircle v-if="community.entryOf(plugin.id)?.update" class="size-3.5 shrink-0 text-sky-400" :aria-label="t('plugins.updateAvailable')" />
      <span class="size-1.5 shrink-0 rounded-full" :class="status(plugin).color" :title="status(plugin).label" />
    </button>

    <CommunityDialog v-model:open="browsing" />
    <PluginDialog v-if="opened" v-model:open="dialogOpen" :plugin="opened" :status="status(opened).label" />
  </div>
</template>
