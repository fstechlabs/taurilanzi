<script setup lang="ts">
import { Download, LoaderCircle } from '@lucide/vue'

const { t } = useI18n()
const { offer, install, remindLater } = useUpdateOffer()
</script>

<template>
  <div
    v-if="offer"
    class="pointer-events-auto rounded-lg border bg-neutral-900 p-3 text-sm shadow-lg shadow-black/40"
    role="status"
    data-update-offer
  >
    <div class="flex items-start gap-2.5">
      <LoaderCircle v-if="offer.installing" class="mt-0.5 size-4 shrink-0 animate-spin text-muted-foreground" />
      <Download v-else class="mt-0.5 size-4 shrink-0 text-emerald-400" />
      <div class="min-w-0 flex-1">
        <p class="font-medium">
          {{ t('shell.update.title', { version: offer.version }) }}
        </p>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {{ offer.installing ? t('shell.update.installing') : t('shell.update.body') }}
        </p>
      </div>
    </div>
    <div v-if="!offer.installing" class="mt-3 flex justify-end gap-2">
      <Button variant="ghost" size="sm" @click="remindLater">
        {{ t('shell.update.later') }}
      </Button>
      <Button size="sm" @click="install">
        {{ t('shell.update.install') }}
      </Button>
    </div>
  </div>
</template>
