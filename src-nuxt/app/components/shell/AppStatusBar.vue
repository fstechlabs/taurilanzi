<script setup lang="ts">
import { os } from '~/lib/os'

const { snapshot, status, version } = useEngine()
const { t } = useI18n()
/** Linux keeps the dock to root until the app's udev rule is in place. */
const accessibility = os === 'linux' ? useAccessibility() : null
</script>

<template>
  <footer class="flex h-7 shrink-0 items-center gap-4 border-t bg-neutral-900/60 px-4 text-[11px] text-muted-foreground">
    <span v-if="status !== 'online'" class="flex items-center gap-1.5">
      <span class="size-1.5 rounded-full bg-amber-500" />
      {{ status === 'connecting' ? t('shell.statusBar.starting') : t('shell.statusBar.engineDown') }}
    </span>
    <template v-else-if="snapshot">
      <span class="flex items-center gap-1.5">
        <span class="size-1.5 rounded-full" :class="snapshot.connected ? 'bg-emerald-500' : 'bg-neutral-500'" />
        <template v-if="snapshot.device">{{ t('shell.statusBar.device', { model: snapshot.device.model, firmware: snapshot.device.firmware }) }}</template>
        <template v-else>{{ t('shell.statusBar.disconnected') }}</template>
      </span>
      <Button
        v-if="accessibility && !snapshot.connected && !accessibility.granted.value"
        variant="link"
        size="sm"
        class="h-auto p-0 text-[11px] text-amber-400 underline"
        data-dock-permission
        @click="accessibility.request()"
      >
        {{ t('shell.statusBar.grantDock') }}
      </Button>
      <span v-if="snapshot.paused" class="text-amber-400">{{ t('shell.statusBar.paused') }}</span>
      <span>{{ t('shell.statusBar.brightness', { value: snapshot.settings.brightness }) }}</span>
      <span>{{ t('shell.statusBar.sleep', { value: snapshot.settings.sleep_minutes === 0 ? t('shell.statusBar.never') : t('shell.statusBar.minutes', { value: snapshot.settings.sleep_minutes }) }) }}</span>
    </template>
    <span v-if="version" class="ml-auto">v{{ version }}</span>
  </footer>
</template>
