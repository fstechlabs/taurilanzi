<script setup lang="ts">
import { thumbnail } from '~/lib/iconify'

const props = defineProps<{ icon: string }>()

const src = computed(() => thumbnail(props.icon))
</script>

<template>
  <img v-if="src" :src="src" alt="" class="size-full">
  <span v-else class="line-clamp-2 text-center text-[10px] break-all text-muted-foreground">
    {{ props.icon.split(':')[1] }}
  </span>
</template>
