<script setup lang="ts">
import type { CatalogEntry } from '~/lib/actions/catalog'
import { actionEntries, typeEntries } from '~/lib/actions/catalog'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{
  /** What is being chosen: the type of a key, one more of its actions, or a pick of a random one. */
  mode: 'type' | 'action' | 'random'
  /** Why an entry cannot be picked right now, if it cannot. */
  unavailable?: (entry: CatalogEntry) => string | null
}>()
const emit = defineEmits<{ pick: [entry: CatalogEntry] }>()
const { t } = useI18n()

const pool = computed(() => {
  if (props.mode === 'type') {
    return typeEntries()
  }
  const entries = actionEntries()
  return props.mode === 'random' ? entries.filter(entry => entry.random) : entries
})

const title = computed(() => t({ type: 'editor.type.choose', action: 'actions.list.add', random: 'actions.list.addRandom' }[props.mode]))

function pick(entry: CatalogEntry) {
  emit('pick', entry)
  open.value = false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex h-[min(80vh,720px)] max-w-[min(92vw,960px)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(92vw,960px)]">
      <DialogHeader class="px-5 pt-5 pb-1">
        <DialogTitle>{{ title }}</DialogTitle>
        <DialogDescription>
          {{ t(props.mode === 'type' ? 'editor.type.pickerDescription' : 'actions.picker.description') }}
        </DialogDescription>
      </DialogHeader>
      <!-- Built again on each opening: a filter left on from last time would hide entries
           without saying why. -->
      <CatalogBrowser
        v-if="open"
        :pool="pool"
        :unavailable="props.unavailable"
        :placeholder="props.mode === 'type' ? t('editor.type.searchPlaceholder') : undefined"
        @pick="pick"
      />
    </DialogContent>
  </Dialog>
</template>
