<script setup lang="ts">
import { keyRadius } from '~/lib/dock'

/** The ghost is as tall as a small key, and as wide as the key dragged. */
const HEIGHT = 56

const { drag } = useKeyDrag()
</script>

<template>
  <Teleport to="body">
    <img
      v-if="drag?.image"
      :src="drag.image"
      alt=""
      class="pointer-events-none fixed z-50 -translate-x-1/2 -translate-y-1/2 opacity-90 shadow-2xl ring-2 ring-sky-400"
      :style="{
        left: `${drag.x}px`,
        top: `${drag.y}px`,
        height: `${HEIGHT}px`,
        width: `${HEIGHT * drag.aspect}px`,
        borderRadius: keyRadius(drag.aspect),
      }"
    >
  </Teleport>
</template>
