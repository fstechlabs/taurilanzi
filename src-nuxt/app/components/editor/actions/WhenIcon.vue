<script setup lang="ts">
import { Clock3, Pointer } from '@lucide/vue'
import type { Gesture } from '~/types/engine'

/** The same finger for every gesture, with a mark that tells them apart. */
const props = defineProps<{ gesture: Gesture }>()
</script>

<template>
  <span class="relative me-1.5 inline-flex size-3.5 shrink-0" aria-hidden="true">
    <Pointer class="size-3.5" />
    <span
      v-if="props.gesture === 'double_press'"
      class="absolute -top-1.5 -right-2 flex size-3 items-center justify-center rounded-full bg-sky-500 text-[8px] leading-none font-bold text-neutral-950 after:content-['2']"
    />
    <span
      v-else-if="props.gesture === 'long_press'"
      class="absolute -top-1.5 -right-2 flex size-3 items-center justify-center rounded-full bg-amber-400 text-neutral-950"
    >
      <Clock3 class="size-2.5" :stroke-width="3" />
    </span>
  </span>
</template>
