<script setup lang="ts">
import { X } from '@lucide/vue'
import { events } from '~/ipc/bindings'

const { t } = useI18n()
const { toasts, show, dismiss } = useToasts()
const { catalog } = usePlugins()

onMounted(() => {
  events.pluginToast.listen(({ payload }) => {
    const plugin = catalog.value?.plugins.find(p => p.id === payload.plugin)
    show(plugin?.name ?? payload.plugin, payload.message)
  })
})
</script>

<template>
  <div class="pointer-events-none fixed right-4 bottom-12 z-[60] flex w-80 flex-col gap-2" aria-live="polite">
    <div
      v-for="toast in toasts"
      :key="toast.id"
      class="pointer-events-auto flex items-start gap-2 rounded-lg border bg-neutral-900 p-3 text-sm shadow-lg shadow-black/40"
      role="status"
      data-toast
    >
      <div class="min-w-0 flex-1">
        <p class="truncate text-xs font-medium text-muted-foreground">
          {{ toast.title }}
        </p>
        <p class="break-words">
          {{ toast.message }}
        </p>
      </div>
      <button type="button" class="text-muted-foreground hover:text-foreground" :aria-label="t('common.toasts.close')" @click="dismiss(toast.id)">
        <X class="size-3.5" />
      </button>
    </div>
    <slot />
  </div>
</template>
