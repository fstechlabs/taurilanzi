<script setup lang="ts">
import { Plus, ServerOff } from '@lucide/vue'
import type { IntegrationKind } from '~/composables/useIntegrations'

/** The server an action or a widget talks to. Without any, it says so and adds one right here. */
const server = defineModel<string>({ required: true })
const props = defineProps<{ kind: IntegrationKind }>()

const { t } = useI18n()
const integrations = useIntegrations()
const adding = ref(false)
const NEW = '+'

const servers = computed(() => integrations.servers(props.kind))
const kindName = computed(() => t(`integrations.kinds.${props.kind === 'obs' ? 'obs' : 'hass'}`))

// A new action takes the first server, or the one just added for it.
watch(servers, (list) => {
  if (list.length && !list.some(s => s.id === server.value)) {
    server.value = list[0]!.id
  }
}, { immediate: true })

const selected = computed({
  get: () => server.value,
  set: (value: string) => {
    if (value === NEW) {
      adding.value = true
    }
    else if (value) {
      server.value = value
    }
  },
})

const dot = (id: string) => ({
  connected: 'bg-emerald-500',
  failed: 'bg-red-500',
  connecting: 'bg-amber-500',
  off: 'bg-neutral-500',
}[integrations.status(id).state])
</script>

<template>
  <div class="flex flex-col gap-1.5" data-server-field>
    <div v-if="!servers.length" class="flex flex-col items-start gap-2 rounded-md border border-dashed p-3">
      <p class="flex items-center gap-2 text-sm">
        <ServerOff class="size-4 text-muted-foreground" /> {{ t('integrations.none', { kind: kindName }) }}
      </p>
      <p class="text-xs text-muted-foreground">
        {{ t('integrations.noneHint', { kind: kindName }) }}
      </p>
      <Button size="sm" @click="adding = true">
        <Plus /> {{ t('integrations.add', { kind: kindName }) }}
      </Button>
    </div>
    <template v-else>
      <Label class="text-xs text-muted-foreground">{{ t('integrations.server') }}</Label>
      <Select v-model="selected">
        <SelectTrigger class="w-full" :aria-label="t('integrations.server')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="s in servers" :key="s.id" :value="s.id">
            <span class="flex items-center gap-2">
              <span class="size-2 shrink-0 rounded-full" :class="dot(s.id)" />
              {{ s.name }}
            </span>
          </SelectItem>
          <SelectSeparator />
          <SelectItem :value="NEW">
            <span class="flex items-center gap-2"><Plus class="size-3.5" /> {{ t('integrations.addAnother') }}</span>
          </SelectItem>
        </SelectContent>
      </Select>
      <p v-if="integrations.status(server).state !== 'connected'" class="text-[11px] text-muted-foreground">
        {{ integrations.status(server).text }}
      </p>
    </template>
    <IntegrationDialog v-model:open="adding" :kind="props.kind" @saved="server = $event" />
  </div>
</template>
