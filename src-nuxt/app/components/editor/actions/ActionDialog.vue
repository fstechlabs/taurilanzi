<script setup lang="ts">
import type { DeepReadonly } from 'vue'
import { actionsError } from '~/composables/useKeyEditor'
import { entryFor } from '~/lib/actions/catalog'
import type { Action, Page } from '~/types/engine'

const open = defineModel<boolean>('open', { required: true })
const action = defineModel<Action>('action', { required: true })
const props = defineProps<{
  page: string
  otherPages: DeepReadonly<Page>[]
  /** What the key is, rather than one more of its actions. */
  face?: boolean
}>()

const { t } = useI18n()
const { snapshot } = useEngine()

const entry = computed(() => entryFor(action.value))
const servers = computed(() => (snapshot.value?.integrations ?? []).map(server => server.id))
const problem = computed(() => actionsError([action.value], servers.value))
const name = computed(() => entry.value?.name ?? (action.value.type === 'plugin' ? t('actions.list.unknownPlugin') : action.value.type))
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl" data-action-dialog>
      <DialogHeader class="border-b px-5 pt-5 pb-4">
        <DialogTitle>{{ name }}</DialogTitle>
        <DialogDescription>
          {{ entry ? t('actions.details.dialogDescription', { description: entry.description.replace(/[.\s]+$/, '') }) : '' }}
        </DialogDescription>
      </DialogHeader>
      <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4">
        <slot />
        <ActionFields v-model="action" :page="props.page" :other-pages="props.otherPages" :face="props.face" />
      </div>
      <div class="flex items-center justify-between gap-3 rounded-b-xl border-t bg-muted/50 px-5 py-3">
        <p class="text-xs" :class="problem ? 'text-destructive' : 'text-muted-foreground'">
          {{ problem ?? t('actions.details.ready') }}
        </p>
        <Button size="sm" @click="open = false">
          {{ t('actions.details.done') }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
