<script setup lang="ts">
defineProps<{ label: string, hint?: string }>()
</script>

<template>
  <div class="flex min-h-14 items-center justify-between gap-6 py-3">
    <div class="min-w-0">
      <p class="text-sm">
        {{ label }}
      </p>
      <p v-if="hint" class="text-xs text-muted-foreground">
        {{ hint }}
      </p>
    </div>
    <div class="flex shrink-0 items-center gap-3">
      <slot />
    </div>
  </div>
</template>
