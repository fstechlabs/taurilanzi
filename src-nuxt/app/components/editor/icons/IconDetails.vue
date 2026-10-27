<script setup lang="ts">
import { Palette } from '@lucide/vue'
import { iconesUrl } from '~/composables/useIconCatalog'
import type { IconCollection } from '~/lib/iconify'

const props = defineProps<{ icon: string, collection?: IconCollection }>()
const { t } = useI18n()
</script>

<template>
  <div class="flex min-w-0 items-center gap-3">
    <div class="flex size-10 shrink-0 items-center justify-center rounded-md bg-neutral-900 p-2 ring-1 ring-neutral-800">
      <IconThumb :icon="props.icon" />
    </div>
    <div class="min-w-0 text-xs">
      <p class="truncate font-mono text-foreground">
        {{ props.icon }}
      </p>
      <p class="flex min-w-0 items-center gap-1.5 truncate text-muted-foreground">
        <span class="truncate">{{ props.collection?.name ?? props.icon.split(':')[0] }}</span>
        <template v-if="props.collection?.license">
          ·
          <ExternalLink v-if="props.collection.license.url" :href="props.collection.license.url">
            {{ props.collection.license.spdx ?? props.collection.license.title }}
          </ExternalLink>
          <span v-else>{{ props.collection.license.title }}</span>
        </template>
        <span v-if="props.collection?.palette" class="inline-flex items-center gap-1">
          · <Palette class="size-3" /> {{ t('icons.details.multicolor') }}
        </span>
        · <ExternalLink :href="iconesUrl(props.icon)">Icones</ExternalLink>
      </p>
    </div>
  </div>
</template>
