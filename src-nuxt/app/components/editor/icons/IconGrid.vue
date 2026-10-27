<script setup lang="ts">
const props = defineProps<{ icons: string[], disabled?: boolean }>()
const emit = defineEmits<{ pick: [icon: string], focus: [icon: string] }>()
</script>

<template>
  <div class="grid grid-cols-10 gap-1.5">
    <button
      v-for="icon in props.icons"
      :key="icon"
      type="button"
      class="flex aspect-square items-center justify-center rounded-md bg-neutral-900 p-2.5 ring-1 ring-neutral-800 transition outline-none hover:bg-neutral-800 hover:ring-primary focus-visible:ring-2 focus-visible:ring-primary"
      :aria-label="icon"
      :title="icon"
      :disabled="props.disabled"
      @click="emit('pick', icon)"
      @mouseenter="emit('focus', icon)"
      @focus="emit('focus', icon)"
    >
      <IconThumb :icon="icon" />
    </button>
  </div>
</template>
