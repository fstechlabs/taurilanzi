<script setup lang="ts">
const { drag } = useKeyDrag()
</script>

<template>
  <Teleport to="body">
    <img
      v-if="drag?.image"
      :src="drag.image"
      alt=""
      class="pointer-events-none fixed z-50 -translate-x-1/2 -translate-y-1/2 opacity-90 shadow-2xl ring-2 ring-sky-400"
      :class="drag.wide ? 'h-14 w-32 rounded-[4%/9%]' : 'size-14 rounded-[9%]'"
      :style="{ left: `${drag.x}px`, top: `${drag.y}px` }"
    >
  </Teleport>
</template>
