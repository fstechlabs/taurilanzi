<script setup lang="ts">
import { Trash2, X } from '@lucide/vue'
import { EdgeLabelRenderer, getBezierPath, useVueFlow } from '@vue-flow/core'
import type { EdgeProps } from '@vue-flow/core'
import type { Point } from '~/lib/link-path'
import { nearestStretch, roundedPath } from '~/lib/link-path'

interface LinkData { color: string, waypoints: Point[], page: string, index: number }

const props = defineProps<EdgeProps<LinkData>>()
const emit = defineEmits<{ remove: [] }>()

/** Screen pixels, whatever the zoom: how wide the line is to the pointer, and how far a press must travel to pull. */
const HIT_WIDTH = 24
const PULL_THRESHOLD = 4
const CORNER = 18

const { selection, selectLink } = useWorkspace()
const { screenToFlowCoordinate, viewport } = useVueFlow()
const actions = usePages()
const contextMenu = useContextMenu()
const { t } = useI18n()

function menu(event: MouseEvent) {
  selectLink(props.data.page, props.data.index)
  contextMenu.open(event, [{ label: t('editor.link.remove'), icon: Trash2, destructive: true, run: () => emit('remove') }])
}

/** Bends while pulling, shown at once and saved on release. */
const draft = ref<Point[] | null>(null)
const waypoints = computed(() => draft.value ?? props.data.waypoints)
const points = computed(() => [
  { x: props.sourceX, y: props.sourceY },
  ...waypoints.value,
  { x: props.targetX, y: props.targetY },
])
const path = computed(() => (waypoints.value.length ? roundedPath(points.value, CORNER) : getBezierPath(props)[0]))
const active = computed(() => {
  const key = selection.value
  return Boolean(key && key.page === props.data.page && key.index === props.data.index)
})
const pulling = ref(false)
/** The remove button sits next to the middle of the line. */
const removeAt = computed(() => {
  const list = points.value
  const a = list[Math.floor((list.length - 1) / 2)]!
  const b = list[Math.floor((list.length - 1) / 2) + 1]!
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }
})

/** The same on screen at any zoom. */
const hitWidth = computed(() => HIT_WIDTH / viewport.value.zoom)
const unzoom = computed(() => 1 / viewport.value.zoom)

let press: { x: number, y: number, bend: number | null } | null = null

/** Pressing on the line pulls a new bend once the pointer moves; pressing on a point moves it. */
function down(event: PointerEvent, bend: number | null) {
  if (event.button !== 0) {
    return
  }
  // Otherwise WebKit starts selecting text across the canvas while dragging.
  event.preventDefault()
  event.stopPropagation()
  press = { x: event.clientX, y: event.clientY, bend }
  window.addEventListener('pointermove', move)
  window.addEventListener('pointerup', up, { once: true })
}

async function removeBend(index: number) {
  await actions.setWaypoints(props.data.page, props.data.index, waypoints.value.filter((_, i) => i !== index))
}

function move(event: PointerEvent) {
  if (!press) {
    return
  }
  if (!pulling.value) {
    if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < PULL_THRESHOLD) {
      return
    }
    const list = [...waypoints.value]
    if (press.bend === null) {
      const at = toCanvas(event)
      press.bend = nearestStretch(points.value, at)
      list.splice(press.bend, 0, at)
    }
    draft.value = list
    pulling.value = true
  }
  const list = [...draft.value!]
  list[press.bend!] = toCanvas(event)
  draft.value = list
}

async function up() {
  window.removeEventListener('pointermove', move)
  press = null
  if (!pulling.value) {
    selectLink(props.data.page, props.data.index)
    return
  }
  pulling.value = false
  try {
    await actions.setWaypoints(props.data.page, props.data.index, waypoints.value)
  }
  catch {
    draft.value = null
  }
}

// The dragged shape stays until the engine hands back the saved one, so the line never jumps back.
watch(() => props.data.waypoints, () => {
  if (!pulling.value) {
    draft.value = null
  }
})

onBeforeUnmount(() => window.removeEventListener('pointermove', move))

function toCanvas(event: MouseEvent): Point {
  return screenToFlowCoordinate({ x: event.clientX, y: event.clientY })
}
</script>

<template>
  <path
    :d="path"
    class="link-path"
    :class="{ active: active || pulling }"
    :style="{ stroke: props.data.color }"
    :marker-end="props.markerEnd"
  />
  <path
    :d="path"
    class="link-hit nopan"
    :class="{ pulling }"
    :style="{ strokeWidth: hitWidth }"
    :data-link="props.id"
    @pointerdown="down($event, null)"
    @contextmenu="menu"
  />
  <EdgeLabelRenderer>
    <button
      v-for="(point, i) in waypoints"
      :key="i"
      type="button"
      class="nodrag nopan absolute size-3.5 cursor-move rounded-full border-2 border-neutral-950 shadow"
      :style="{
        transform: `translate(${point.x}px, ${point.y}px) translate(-50%, -50%) scale(${unzoom})`,
        background: props.data.color,
        pointerEvents: 'all',
      }"
      :aria-label="t('editor.link.bend', { number: i + 1 })"
      :title="t('editor.link.bendHint')"
      :data-bend="i"
      @pointerdown="down($event, i)"
      @dblclick.stop="removeBend(i)"
    />
    <button
      v-if="active && !pulling"
      type="button"
      class="nodrag nopan absolute flex size-6 items-center justify-center rounded-full border bg-neutral-900 text-neutral-200 shadow hover:text-destructive"
      :style="{
        transform: `translate(${removeAt.x}px, ${removeAt.y}px) scale(${unzoom}) translate(-50%, -150%)`,
        pointerEvents: 'all',
        borderColor: props.data.color,
      }"
      :aria-label="t('editor.link.remove')"
      :title="t('editor.link.removeHint')"
      @click="emit('remove')"
    >
      <X class="size-3.5" />
    </button>
  </EdgeLabelRenderer>
</template>
