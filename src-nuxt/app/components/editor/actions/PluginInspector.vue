<script setup lang="ts">
import { ExternalLink, TriangleAlert } from '@lucide/vue'
import { KEY_TARGET } from '~/lib/actions/key-draft'
import { findAction, inspectorUrl } from '~/lib/actions/plugins'
import { openExternal } from '~/lib/external'

const props = defineProps<{ action: string, id: string, settings?: unknown }>()

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
    url.value = inspectorUrl(value, action.plugin, action.action, target?.value?.index ?? 0, props.id, props.settings)
  }
})
const running = computed(() => found.value?.plugin.connected ?? false)
/** Stream Dock shows these pages as they are, in a column this wide over its own grey. */
const streamDeck = computed(() => found.value?.plugin.protocol === 'stream_deck')
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
      <Button
        v-if="found.plugin.website"
        variant="ghost"
        size="sm"
        class="self-start text-muted-foreground"
        data-plugin-website
        @click="openExternal(found.plugin.website)"
      >
        <ExternalLink /> {{ t('actions.pluginInspector.website', { name: found.plugin.name }) }}
      </Button>
      <p v-if="!found.action.inspector" class="text-sm text-muted-foreground">
        {{ t('actions.pluginInspector.noSettings') }}
      </p>
      <iframe
        v-else-if="url"
        :src="url"
        :title="t('actions.pluginInspector.settingsOf', { name: found.action.name })"
        class="h-[60vh] rounded-lg"
        :class="streamDeck ? 'mx-auto block w-[430px] max-w-full bg-[#2d2d2d]' : 'w-full bg-transparent'"
        data-plugin-inspector
      />
    </template>
  </div>
</template>
