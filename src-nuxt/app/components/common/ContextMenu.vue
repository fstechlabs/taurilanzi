<script setup lang="ts">
const { menu, close } = useContextMenu()

const root = useTemplateRef<HTMLDivElement>('root')
const position = ref({ x: 0, y: 0 })

// Kept inside the window, opening up or left when there is no room.
watch(menu, async (value) => {
  if (!value) {
    return
  }
  position.value = { x: value.x, y: value.y }
  await nextTick()
  const box = root.value?.getBoundingClientRect()
  if (box) {
    position.value = {
      x: Math.min(value.x, window.innerWidth - box.width - 8),
      y: value.y + box.height > window.innerHeight - 8 ? value.y - box.height : value.y,
    }
  }
  root.value?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus()
})

function outside(event: PointerEvent) {
  if (menu.value && !root.value?.contains(event.target as Node)) {
    close()
  }
}

function key(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    close()
  }
}

function run(item: { run: () => unknown }) {
  close()
  item.run()
}

onMounted(() => {
  window.addEventListener('pointerdown', outside, true)
  window.addEventListener('keydown', key)
  window.addEventListener('blur', close)
  window.addEventListener('wheel', close, { passive: true })
})
onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', outside, true)
  window.removeEventListener('keydown', key)
  window.removeEventListener('blur', close)
  window.removeEventListener('wheel', close)
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="menu"
      ref="root"
      role="menu"
      class="fixed z-50 min-w-48 rounded-lg border bg-popover p-1 text-sm text-popover-foreground shadow-md"
      :style="{ left: `${position.x}px`, top: `${position.y}px` }"
      @contextmenu.prevent
    >
      <template v-for="(item, i) in menu.items" :key="i">
        <div v-if="item.separated" class="-mx-1 my-1 h-px bg-border" />
        <button
          type="button"
          role="menuitem"
          :disabled="item.disabled"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none hover:bg-accent focus-visible:bg-accent disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4"
          :class="item.destructive ? 'text-destructive hover:bg-destructive/10 focus-visible:bg-destructive/10' : ''"
          @click="run(item)"
        >
          <component :is="item.icon" v-if="item.icon" />
          {{ item.label }}
        </button>
      </template>
    </div>
  </Teleport>
</template>
