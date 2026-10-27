<script setup lang="ts">
import { ChevronDown, CircleUserRound, MonitorSmartphone, Settings } from '@lucide/vue'

const { snapshot } = useEngine()
const { t } = useI18n()
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
      <div
        class="flex cursor-default items-center gap-2 rounded px-2 py-1.5 text-muted-foreground/60"
        aria-disabled="true"
        :title="t('shell.sidebar.accountHint')"
      >
        <CircleUserRound class="size-4" />
        <span class="flex-1">{{ t('shell.sidebar.account') }}</span>
        <Badge variant="secondary" class="text-[10px]">
          {{ t('shell.sidebar.soon') }}
        </Badge>
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
