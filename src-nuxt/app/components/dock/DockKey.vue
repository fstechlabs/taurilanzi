<script setup lang="ts">
import { Trash2 } from '@lucide/vue'

const props = defineProps<{
  page: string
  index: number
  imageUrl: string | null
  configured: boolean
  pressed: boolean
  selected: boolean
  wide?: boolean
  compact?: boolean
}>()

const emit = defineEmits<{ select: [index: number] }>()

const { t } = useI18n()
const keyDrag = useKeyDrag()
const contextMenu = useContextMenu()
const pages = usePages()
const slot = computed(() => ({ page: props.page, index: props.index }))
const over = computed(() => keyDrag.isOver(slot.value))
const lifted = computed(() => keyDrag.isDragging(slot.value))

function menu(event: MouseEvent) {
  contextMenu.open(event, props.configured
    ? [{ label: t('dock.key.remove'), icon: Trash2, destructive: true, run: () => pages.removeKey(props.page, props.index) }]
    : [])
}

function click() {
  if (!keyDrag.consumeClick()) {
    emit('select', props.index)
  }
}
</script>

<template>
  <div
    class="relative"
    :class="props.wide ? 'col-span-2 aspect-[452/196]' : 'aspect-square'"
    data-key-slot
    :data-page="props.page"
    :data-index="props.index"
  >
    <button
      type="button"
      :aria-label="t('dock.key.label', { n: props.index + 1 })"
      class="group nopan absolute inset-0 touch-none overflow-hidden bg-neutral-900 ring-offset-black transition outline-none select-none focus-visible:ring-2 focus-visible:ring-ring"
      :class="[
        props.wide ? 'rounded-[4%/9%]' : 'rounded-[9%]',
        props.compact ? 'ring-offset-1' : 'ring-offset-2',
        over ? 'ring-2 ring-sky-400' : props.selected ? 'ring-2 ring-white' : 'hover:ring-2 hover:ring-neutral-600',
        props.pressed ? 'scale-95 brightness-125' : '',
        lifted ? 'opacity-30' : '',
        props.configured ? 'cursor-grab' : '',
      ]"
      @pointerdown="keyDrag.press($event, slot, props.configured ? props.imageUrl : null, Boolean(props.wide))"
      @pointermove="keyDrag.move"
      @pointerup="keyDrag.release"
      @pointercancel="keyDrag.release"
      @click="click"
      @contextmenu="menu"
    >
      <img
        v-if="props.configured && props.imageUrl"
        :src="props.imageUrl"
        alt=""
        class="pointer-events-none absolute inset-0 size-full object-fill"
        draggable="false"
      >
      <span
        v-else-if="!props.compact"
        class="absolute inset-0 flex items-center justify-center text-xs text-neutral-600 group-hover:text-neutral-400"
      >
        {{ t('dock.key.empty') }}
      </span>
    </button>
    <slot />
  </div>
</template>
