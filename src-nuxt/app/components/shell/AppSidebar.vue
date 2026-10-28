<script setup lang="ts">
import { ChevronDown, CircleUserRound, Cloud, MonitorSmartphone, RefreshCw, Settings } from '@lucide/vue'

const { snapshot } = useEngine()
const { t } = useI18n()
const cloud = useCloud()

/** Shown only when there is something to upload or download, and nothing stops it. */
const canSync = computed(() => {
  const status = cloud.status.value
  return Boolean(status?.signed_in && !status.paused && (status.outgoing || status.incoming || status.syncing))
})
const syncTitle = computed(() => {
  const status = cloud.status.value
  if (status?.syncing) {
    return t('shell.sidebar.syncing')
  }
  return status?.outgoing ? t('shell.sidebar.syncOutgoing') : t('shell.sidebar.syncIncoming')
})
/** How this computer is doing with the cloud: red with a problem, orange with changes to sync, green otherwise. */
const mark = computed(() => {
  const status = cloud.status.value
  if (!status?.signed_in) {
    return null
  }
  if (status.problem) {
    return { state: 'problem', color: 'text-red-500', text: status.problem }
  }
  if (status.outgoing || status.incoming) {
    return { state: 'pending', color: 'text-amber-500', text: syncTitle.value }
  }
  return { state: 'synced', color: 'text-emerald-500', text: t('shell.sidebar.accountConnected') }
})
</script>

<template>
  <aside class="flex w-64 shrink-0 flex-col border-r bg-neutral-900/40">
    <ScrollArea class="min-h-0 flex-1">
      <div class="p-2 text-sm">
        <p class="px-2 pt-2 pb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
          {{ t('shell.sidebar.devices') }}
        </p>
        <div v-if="snapshot" class="mb-1">
          <div class="flex items-center gap-1.5 rounded px-2 py-1">
            <ChevronDown class="size-3.5" />
            <MonitorSmartphone class="size-4" />
            <span class="flex-1">{{ snapshot.device?.model ?? 'D200' }}</span>
            <span class="size-1.5 rounded-full" :class="snapshot.connected ? 'bg-emerald-500' : 'bg-neutral-500'" :title="snapshot.connected ? t('shell.sidebar.connected') : t('shell.sidebar.disconnected')" />
          </div>
          <ProfileList class="ml-4 border-l border-neutral-800 pl-2" />
        </div>

        <PluginList />
      </div>
    </ScrollArea>

    <nav class="flex flex-col gap-0.5 border-t border-neutral-800 p-2 text-sm">
      <div class="group flex items-center gap-1 rounded pr-1.5 text-muted-foreground hover:bg-neutral-800/60 hover:text-foreground">
        <NuxtLink
          :to="{ path: '/settings', query: { tab: 'cloud' } }"
          class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5"
          :title="cloud.signedIn.value ? undefined : t('shell.sidebar.accountHint')"
          data-sidebar-account
        >
          <CircleUserRound class="size-4" />
          <span class="flex-1">{{ t('shell.sidebar.account') }}</span>
          <Badge v-if="!cloud.signedIn.value" variant="secondary" class="text-[10px]">
            {{ t('shell.sidebar.connect') }}
          </Badge>
        </NuxtLink>
        <Button
          v-if="canSync"
          variant="outline"
          size="sm"
          class="h-7 shrink-0 px-2 text-xs"
          :disabled="cloud.status.value?.syncing"
          :title="syncTitle"
          data-sidebar-sync
          @click="cloud.syncNow()"
        >
          <RefreshCw class="size-3.5" :class="cloud.status.value?.syncing ? 'animate-spin' : ''" />
          {{ t('shell.sidebar.sync') }}
        </Button>
        <NuxtLink
          v-if="mark"
          :to="{ path: '/settings', query: { tab: 'cloud' } }"
          class="shrink-0 py-1.5"
          :class="mark.color"
          :title="mark.text"
          :aria-label="mark.text"
          :data-sidebar-cloud="mark.state"
        >
          <Cloud class="size-4" />
        </NuxtLink>
      </div>
      <NuxtLink
        to="/settings"
        class="flex items-center gap-2 rounded px-2 py-1.5 text-muted-foreground hover:bg-neutral-800/60 hover:text-foreground"
        active-class="bg-neutral-800 text-foreground"
      >
        <Settings class="size-4" />
        {{ t('shell.sidebar.settings') }}
      </NuxtLink>
    </nav>
  </aside>
</template>
