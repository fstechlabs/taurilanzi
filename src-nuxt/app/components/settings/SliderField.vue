<script setup lang="ts">
const value = defineModel<number>({ required: true })
defineProps<{ label: string, min: number, max: number, step?: number, unit?: string }>()

const model = computed({
  get: () => [value.value],
  set: ([next]) => {
    if (next !== undefined) {
      value.value = next
    }
  },
})
</script>

<template>
  <div class="flex w-72 items-center gap-3">
    <Slider v-model="model" :min="min" :max="max" :step="step ?? 1" :aria-label="label" class="flex-1" />
    <span class="w-16 text-right text-xs tabular-nums">{{ value }}{{ unit }}</span>
  </div>
</template>
