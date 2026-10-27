<script setup lang="ts">
defineProps<{ src: string | null, name: string, selected: boolean }>()
const emit = defineEmits<{ select: [] }>()
</script>

<template>
  <button
    type="button"
    class="flex flex-col gap-1.5 rounded-lg p-1.5 text-left ring-1 transition-colors"
    :class="selected ? 'bg-neutral-800/60 ring-2 ring-primary' : 'ring-neutral-800 hover:bg-neutral-800/40'"
    :aria-pressed="selected"
    :aria-label="name"
    @click="emit('select')"
  >
    <img v-if="src" :src="src" alt="" draggable="false" class="aspect-[1220/708] w-full rounded bg-black object-cover">
    <div v-else class="flex aspect-[1220/708] w-full items-center justify-center rounded bg-neutral-900 text-xs text-muted-foreground">
      <slot />
    </div>
    <span class="px-1 text-xs">{{ name }}</span>
  </button>
</template>
