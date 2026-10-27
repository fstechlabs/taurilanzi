<script setup lang="ts">
import { ExternalLink as ExternalIcon } from '@lucide/vue'
import { openExternal } from '~/lib/external'

const props = defineProps<{ href: string }>()
</script>

<template>
  <a
    :href="props.href"
    class="inline-flex items-center gap-0.5 font-medium text-foreground underline-offset-2 hover:underline"
    @click.prevent="openExternal(props.href)"
  >
    <slot /><ExternalIcon class="size-3 opacity-60" />
  </a>
</template>
