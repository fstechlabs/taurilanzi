<script setup lang="ts">
import { TriangleAlert } from '@lucide/vue'
import { KEY_TARGET } from '~/lib/actions/key-draft'
import { findAction, inspectorUrl } from '~/lib/actions/plugins'

const props = defineProps<{ action: string, id: string }>()

const { t } = useI18n()
const { catalog } = usePlugins()
const target = inject(KEY_TARGET, null)

const found = computed(() => findAction(catalog.value, props.action))
/** Built once per dialog: a new address would reload the page and lose what is typed in it. */
const url = ref<string | null>(null)
watchEffect(() => {
  const value = catalog.value
  const action = found.value
  if (!url.value && value && action) {
    url.value = inspectorUrl(value, action.plugin, action.action, target?.value?.index ?? 0, props.id)
  }
})
const running = computed(() => found.value?.plugin.connected ?? false)
</script>

<template>
  <div class="flex flex-col gap-3">
    <p v-if="!found" class="text-sm text-muted-foreground">
      {{ t('actions.pluginInspector.notInstalled') }}
    </p>
    <template v-else>
      <div v-if="!running" class="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-200">
        <TriangleAlert class="mt-0.5 size-4 shrink-0" />
        {{ found.plugin.run.state === 'unavailable' ? found.plugin.run.reason : t('actions.pluginInspector.notRunning') }}
      </div>
      <p v-if="!found.action.inspector" class="text-sm text-muted-foreground">
        {{ t('actions.pluginInspector.noSettings') }}
      </p>
      <iframe
        v-else-if="url"
        :src="url"
        :title="t('actions.pluginInspector.settingsOf', { name: found.action.name })"
        class="h-[60vh] w-full rounded-md border bg-neutral-950"
        data-plugin-inspector
      />
    </template>
  </div>
</template>
