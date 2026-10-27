<script setup lang="ts">
import { Search } from '@lucide/vue'

const { t } = useI18n()
const { name } = useEngine()

/** Right click only ever opens the app's own menus, never the browser's. */
function blockNativeMenu(event: MouseEvent) {
  event.preventDefault()
}

useHistory()

onMounted(() => window.addEventListener('contextmenu', blockNativeMenu))
onBeforeUnmount(() => window.removeEventListener('contextmenu', blockNativeMenu))
</script>

<template>
  <div class="flex h-screen flex-col bg-neutral-950 text-foreground">
    <header class="flex h-11 shrink-0 items-center gap-3 border-b bg-neutral-900/60 px-4 text-sm">
      <span class="font-semibold">{{ name }}</span>
      <div class="relative ml-4 w-80">
        <Search class="absolute top-2 left-2.5 size-4 text-muted-foreground" />
        <Input :placeholder="t('shell.header.search')" class="h-8 pl-8" disabled />
      </div>
    </header>
    <div class="flex min-h-0 flex-1">
      <AppSidebar />
      <slot />
    </div>
    <AppStatusBar />
    <ContextMenu />
    <AppToasts>
      <UpdateOffer />
    </AppToasts>
  </div>
</template>
