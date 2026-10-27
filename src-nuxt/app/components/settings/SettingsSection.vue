<script setup lang="ts">
defineProps<{ title?: string, description?: string }>()
</script>

<template>
  <section class="flex flex-col gap-3" :aria-label="title">
    <div v-if="title || description">
      <h2 v-if="title" class="text-sm font-semibold">
        {{ title }}
      </h2>
      <p v-if="description" class="text-xs text-muted-foreground">
        {{ description }}
      </p>
    </div>
    <div class="divide-y divide-neutral-800 rounded-lg bg-neutral-900/50 px-4 ring-1 ring-neutral-800">
      <slot />
    </div>
  </section>
</template>
