<script setup lang="ts">
import { ArrowUpCircle, FolderOpen, LoaderCircle, Puzzle, RotateCcw, Trash2 } from '@lucide/vue'
import type { DeepReadonly } from 'vue'
import { pluginImage } from '~/lib/actions/plugins'
import type { PluginInfo } from '~/types/engine'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ plugin: DeepReadonly<PluginInfo>, status: string }>()

const { restart, uninstall, showFolder } = usePlugins()
const community = useCommunityPlugins()
const { show } = useToasts()
const { t } = useI18n()

onMounted(() => community.load())

const offered = computed(() => community.entryOf(props.plugin.id))
const updating = computed(() => Boolean(offered.value && community.installing.value.has(offered.value.id)))

async function update() {
  if (!offered.value) {
    return
  }
  try {
    await community.install(offered.value.id)
    const version = offered.value?.version
    show(t('plugins.updated'), version
      ? t('plugins.dialog.updatedTo', { name: props.plugin.name, version })
      : t('plugins.dialog.updatedLatest', { name: props.plugin.name }))
  }
  catch (err) {
    show(t('plugins.dialog.updateFailed'), err instanceof Error ? err.message : String(err))
  }
}

const runs = computed(() => !['passive', 'unavailable'].includes(props.plugin.run.state))
/** Removing cannot be undone, so it asks once more on the same button. */
const confirming = ref(false)
watch(() => props.plugin.id, () => {
  confirming.value = false
})

async function remove() {
  if (!confirming.value) {
    confirming.value = true
    return
  }
  try {
    await uninstall(props.plugin.id)
    open.value = false
  }
  catch (err) {
    show(t('plugins.dialog.removeFailed'), err instanceof Error ? err.message : String(err))
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg" data-plugin-dialog>
      <DialogHeader class="flex-row items-center gap-3 border-b px-5 pt-5 pb-4">
        <img v-if="props.plugin.icon" :src="pluginImage(props.plugin.id, props.plugin.icon)" alt="" class="size-10 shrink-0 rounded-md">
        <Puzzle v-else class="size-10 shrink-0 text-muted-foreground" />
        <div class="min-w-0 flex-1">
          <DialogTitle class="truncate">
            {{ props.plugin.name }}
          </DialogTitle>
          <DialogDescription>
            {{ [props.plugin.author, props.plugin.version && t('plugins.version', { version: props.plugin.version })].filter(Boolean).join(' · ') }}
          </DialogDescription>
        </div>
      </DialogHeader>
      <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4 text-sm">
        <p v-if="props.plugin.description" class="text-muted-foreground">
          {{ props.plugin.description }}
        </p>
        <div class="flex flex-wrap gap-1.5">
          <Badge variant="outline" data-plugin-status>
            {{ props.status }}
          </Badge>
        </div>
        <div class="flex flex-col gap-1.5">
          <p class="text-xs font-medium text-muted-foreground">
            {{ t('plugins.dialog.actions') }}
          </p>
          <div v-for="action in props.plugin.actions" :key="action.uuid" class="flex items-center gap-2.5 rounded-md bg-neutral-900/60 px-2.5 py-1.5">
            <img v-if="action.icon" :src="pluginImage(props.plugin.id, action.icon)" alt="" class="size-7 shrink-0 rounded">
            <div class="min-w-0 flex-1">
              <p class="truncate">
                {{ action.name }}
              </p>
              <p v-if="action.tooltip || !action.supported" class="truncate text-[11px] text-muted-foreground">
                {{ action.supported ? action.tooltip : t('plugins.dialog.otherModels') }}
              </p>
            </div>
          </div>
        </div>
      </div>
      <div class="flex items-center gap-2 rounded-b-xl border-t bg-muted/50 px-5 py-3">
        <Button v-if="offered?.update" size="sm" :disabled="updating" @click="update">
          <LoaderCircle v-if="updating" class="animate-spin" />
          <ArrowUpCircle v-else />
          {{ updating ? t('plugins.dialog.updating') : t('plugins.updateTo', { version: offered.version }) }}
        </Button>
        <Button v-if="runs" variant="outline" size="sm" @click="restart(props.plugin.id)">
          <RotateCcw /> {{ t('plugins.dialog.restart') }}
        </Button>
        <Button variant="outline" size="sm" @click="showFolder()">
          <FolderOpen /> {{ t('plugins.dialog.folder') }}
        </Button>
        <span class="flex-1" />
        <Button :variant="confirming ? 'destructive' : 'ghost'" size="sm" @click="remove">
          <Trash2 /> {{ confirming ? t('plugins.dialog.confirmRemove') : t('plugins.dialog.remove') }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
