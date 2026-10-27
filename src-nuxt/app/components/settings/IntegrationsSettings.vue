<script setup lang="ts">
import { Pencil, Plus } from '@lucide/vue'
import type { Integration } from '~/types/engine'
import type { IntegrationKind } from '~/composables/useIntegrations'

/** The servers of OBS and of Home Assistant, each set up in the same dialog as from an action. */
const { t } = useI18n()
const integrations = useIntegrations()

const KINDS: IntegrationKind[] = ['obs', 'home_assistant']
const editing = ref<{ kind: IntegrationKind, server: Integration | null } | null>(null)
const open = computed({
  get: () => Boolean(editing.value),
  set: (value: boolean) => {
    if (!value) {
      editing.value = null
    }
  },
})

function key(kind: IntegrationKind) {
  return kind === 'obs' ? 'obs' : 'hass'
}

function address(server: Integration) {
  return server.kind === 'obs' ? `${server.host || '127.0.0.1'}:${server.port || 4455}` : server.url
}

const dot = (id: string) => ({
  connected: 'bg-emerald-500',
  failed: 'bg-red-500',
  connecting: 'bg-amber-500',
  off: 'bg-neutral-500',
}[integrations.status(id).state])
</script>

<template>
  <div class="flex flex-col gap-6">
    <SettingsSection v-for="kind in KINDS" :key="kind" :title="t(`integrations.kinds.${key(kind)}`)" :description="t(`integrations.about.${key(kind)}`)" :data-integrations="kind">
      <div
        v-for="server in integrations.servers(kind)"
        :key="server.id"
        class="flex items-center gap-3 border-b py-3 last:border-b-0"
        :data-integration="server.name"
      >
        <span class="size-2 shrink-0 rounded-full" :class="dot(server.id)" />
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-medium">
            {{ server.name }}
          </p>
          <p class="truncate text-xs text-muted-foreground">
            {{ address(server) }} · {{ integrations.status(server.id).text }}
          </p>
        </div>
        <Button variant="outline" size="sm" :aria-label="t('integrations.editServer', { name: server.name })" @click="editing = { kind, server }">
          <Pencil /> {{ t('integrations.editButton') }}
        </Button>
      </div>
      <p v-if="!integrations.servers(kind).length" class="py-3 text-xs text-muted-foreground">
        {{ t('integrations.empty', { kind: t(`integrations.kinds.${key(kind)}`) }) }}
      </p>
      <div class="py-3">
        <Button variant="outline" size="sm" @click="editing = { kind, server: null }">
          <Plus /> {{ t('integrations.add', { kind: t(`integrations.kinds.${key(kind)}`) }) }}
        </Button>
      </div>
    </SettingsSection>
    <IntegrationDialog v-if="editing" v-model:open="open" :kind="editing.kind" :server="editing.server" />
  </div>
</template>
