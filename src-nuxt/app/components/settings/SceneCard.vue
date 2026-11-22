<script setup lang="ts">
defineProps<{ src: string | null, name: string, selected: boolean }>()
const emit = defineEmits<{ select: [] }>()
const { model } = useEngine()
/** The shape of the panel of the dock the editor shows. */
const panel = computed(() => model.value ? `${model.value.panel.width} / ${model.value.panel.height}` : '1220 / 708')
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
    <img v-if="src" :src="src" alt="" draggable="false" class="w-full rounded bg-black object-cover" :style="{ aspectRatio: panel }">
    <div v-else class="flex w-full items-center justify-center rounded bg-neutral-900 text-xs text-muted-foreground" :style="{ aspectRatio: panel }">
      <slot />
    </div>
    <span class="px-1 text-xs">{{ name }}</span>
  </button>
</template>
