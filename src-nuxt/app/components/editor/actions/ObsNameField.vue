<script setup lang="ts">
import { ChevronDown } from '@lucide/vue'
import { commands } from '~/ipc/bindings'
import type { ObsList } from '~/types/engine'

/** A name as OBS shows it: typed by hand, or picked from OBS's list while it is connected. */
const name = defineModel<string>({ required: true })
const props = defineProps<{ server: string, list: ObsList, label: string, placeholder?: string }>()

const { t } = useI18n()
const { snapshot } = useEngine()
const connected = computed(() => snapshot.value?.obs[props.server]?.status.state === 'connected')
const names = ref<string[]>([])
const loading = ref(false)
const failed = ref(false)

async function load(open: boolean) {
  if (!open) {
    return
  }
  loading.value = true
  failed.value = false
  try {
    names.value = await call(commands.obsNames(props.server, props.list))
  }
  catch {
    names.value = []
    failed.value = true
  }
  finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="flex items-center gap-1.5">
    <Input
      :model-value="name"
      :placeholder="props.placeholder ?? props.label"
      :aria-label="props.label"
      class="flex-1"
      @update:model-value="name = String($event)"
    />
    <DropdownMenu @update:open="load">
      <DropdownMenuTrigger as-child>
        <Button
          variant="outline"
          size="icon"
          :disabled="!connected"
          :aria-label="t('actions.obs.names.pick', { label: props.label })"
          :title="t(connected ? 'actions.obs.names.pickTitle' : 'actions.obs.names.connectTitle')"
        >
          <ChevronDown />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" class="max-h-72 w-60 overflow-y-auto">
        <DropdownMenuItem v-if="loading" disabled>
          {{ t('actions.obs.names.reading') }}
        </DropdownMenuItem>
        <DropdownMenuItem v-else-if="!names.length" disabled>
          {{ t(failed ? 'actions.obs.names.readFailed' : 'actions.obs.names.none') }}
        </DropdownMenuItem>
        <template v-else>
          <DropdownMenuItem v-for="item in names" :key="item" @select="name = item">
            {{ item }}
          </DropdownMenuItem>
        </template>
      </DropdownMenuContent>
    </DropdownMenu>
  </div>
</template>
