<script setup lang="ts">
import { FlipHorizontal2, FlipVertical2, Loader2, Maximize, Minimize, Minus, Plus, RotateCcw, RotateCw, Undo2 } from '@lucide/vue'
import { useElementSize } from '@vueuse/core'
import type { ImageDraft } from '~/composables/useKeyEditor'
import type { SourceImage } from '~/lib/image-crop'
import type { Crop } from '~/types/engine'
import {
  bake, clampZoom, fillZoom, fitZoom, identityCrop, loadSource, minZoom, outputSize, placement,
  ZOOM_MAX, zoomAround,
} from '~/lib/image-crop'

const open = defineModel<boolean>('open', { required: true })
const image = defineModel<ImageDraft>('image', { required: true })
const props = defineProps<{
  /** A key, the wide key, or the whole panel for the screensaver. */
  shape: 'key' | 'wide' | 'screensaver'
  background: string
}>()

const { assetBlob, upload } = useEngine()
const { t } = useI18n()

const PADDING = 48
/** Pixels within which a dragged image snaps to the center lines. */
const SNAP = 6
const ZOOM_STEP = 1.15

const stage = useTemplateRef<HTMLDivElement>('stage')
const { width: stageWidth, height: stageHeight } = useElementSize(stage)

const source = shallowRef<SourceImage | null>(null)
const shownUrl = ref<string | null>(null)
const crop = ref<Crop>(identityCrop())
const status = ref<'loading' | 'ready' | 'saving' | 'error'>('loading')
const failed = ref(false)
const moving = ref(false)
const snapped = ref({ x: false, y: false })

const PANEL = { width: 1220, height: 708 }
/** The gaps between keys, in panel pixels: two rows across and the columns between keys. */
const PANEL_GAPS = [
  { x: 0, y: 196, width: 1220, height: 60 },
  { x: 0, y: 452, width: 1220, height: 60 },
  ...[196, 452, 708, 964].flatMap(x => [
    { x, y: 0, width: 60, height: 196 },
    { x, y: 256, width: 60, height: 196 },
  ]),
  ...[196, 452].map(x => ({ x, y: 512, width: 60, height: 196 })),
]
const screensaver = computed(() => props.shape === 'screensaver')

const aspect = computed(() => {
  switch (props.shape) {
    case 'wide': return 452 / 196
    case 'screensaver': return PANEL.width / PANEL.height
    default: return 1
  }
})
/** The key's image area in stage pixels. The image keeps its position relative to it. */
const frame = computed(() => {
  const width = Math.max(0, Math.min(stageWidth.value - PADDING * 2, (stageHeight.value - PADDING * 2) * aspect.value))
  return { width, height: width / aspect.value }
})
const unit = computed(() => ({ width: aspect.value, height: 1 }))
const natural = computed(() => source.value?.natural ?? { width: 1, height: 1 })
const output = computed(() => screensaver.value ? PANEL : outputSize(aspect.value))
const gapStyles = computed(() => screensaver.value
  ? PANEL_GAPS.map(gap => ({
      left: `${gap.x / PANEL.width * 100}%`,
      top: `${gap.y / PANEL.height * 100}%`,
      width: `${gap.width / PANEL.width * 100}%`,
      height: `${gap.height / PANEL.height * 100}%`,
    }))
  : [])

const frameStyle = computed(() => ({
  left: `${(stageWidth.value - frame.value.width) / 2}px`,
  top: `${(stageHeight.value - frame.value.height) / 2}px`,
  width: `${frame.value.width}px`,
  height: `${frame.value.height}px`,
  borderRadius: screensaver.value ? '6px' : `${frame.value.height * 0.1}px`,
}))
const imageStyle = computed(() => {
  const p = placement(crop.value, natural.value, frame.value)
  return {
    width: `${p.width}px`,
    height: `${p.height}px`,
    transform: `translate(${p.centerX}px, ${p.centerY}px) rotate(${crop.value.rotation}deg) scale(${p.flipX}, ${p.flipY}) translate(-50%, -50%)`,
  }
})

const quarter = computed(() => Math.round(crop.value.rotation / 90))
const straighten = computed({
  get: () => [round(crop.value.rotation - quarter.value * 90, 10)],
  set: ([value]) => setRotation(quarter.value * 90 + (value ?? 0)),
})
const zoomSlider = computed({
  get: () => [Math.log(crop.value.zoom)],
  set: ([value]) => zoomTo(Math.exp(value ?? 0)),
})
const zoomRange = computed(() => ({ min: Math.log(minZoom(natural.value, unit.value)), max: Math.log(ZOOM_MAX) }))
/** Part of the frame is left empty, so the key background shows through there. */
const gaps = computed(() => !covers(crop.value))

watch(open, async (visible) => {
  if (!visible) {
    release()
    return
  }
  status.value = 'loading'
  failed.value = false
  try {
    const loaded = await loadSource(await assetBlob(image.value.source))
    source.value = loaded
    shownUrl.value = shownImage(loaded)
    crop.value = image.value.crop ? { ...image.value.crop } : startCrop(loaded)
    status.value = 'ready'
    await nextTick()
    stage.value?.focus()
  }
  catch {
    status.value = 'error'
  }
}, { immediate: true })

onBeforeUnmount(release)

function startCrop(loaded: SourceImage): Crop {
  const zoom = image.value.fit === 'contain' ? fitZoom(loaded.natural, unit.value, 0) : 1
  return { ...identityCrop(), zoom }
}

/** Icons are painted with `currentColor`, which an `<img>` shows black: show them tinted. */
function shownImage(loaded: SourceImage) {
  if (loaded.svg === null || !image.value.tint) {
    return loaded.url
  }
  const svg = loaded.svg.replaceAll('currentColor', image.value.tint)
  return URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }))
}

function release() {
  if (shownUrl.value && shownUrl.value !== source.value?.url) {
    URL.revokeObjectURL(shownUrl.value)
  }
  if (source.value) {
    URL.revokeObjectURL(source.value.url)
  }
  source.value = null
  shownUrl.value = null
}

function covers(c: Crop) {
  const p = placement(c, natural.value, unit.value)
  const r = -c.rotation * Math.PI / 180
  const corners = [[-1, -1], [1, -1], [1, 1], [-1, 1]] as const
  return corners.every(([sx, sy]) => {
    const dx = sx * unit.value.width / 2 - (p.centerX - unit.value.width / 2)
    const dy = sy * unit.value.height / 2 - (p.centerY - unit.value.height / 2)
    const u = dx * Math.cos(r) - dy * Math.sin(r)
    const v = dx * Math.sin(r) + dy * Math.cos(r)
    return Math.abs(u) <= p.width / 2 + 1e-6 && Math.abs(v) <= p.height / 2 + 1e-6
  })
}

function zoomTo(zoom: number, point = { x: 0, y: 0 }) {
  const next = clampZoom(zoom, natural.value, unit.value)
  crop.value = zoomAround(crop.value, round(next, 1000), point, frame.value)
}

/** Rotating keeps the frame filled if it was filled, like straightening a photo. */
function setRotation(rotation: number) {
  const normalized = ((rotation % 360) + 540) % 360 - 180
  const current = crop.value
  const next = { ...current, rotation: normalized === -180 ? 180 : normalized }
  if (covers(current)) {
    const fitted = Math.abs(current.zoom - fillZoom(natural.value, unit.value, current.rotation)) < 1e-3
    const needed = fillZoom(natural.value, unit.value, next.rotation)
    next.zoom = fitted ? needed : Math.max(current.zoom, needed)
  }
  crop.value = next
}

/** Mirrors what is on screen, whatever the rotation. */
function flip(axis: 'x' | 'y') {
  const c = crop.value
  crop.value = axis === 'x'
    ? { ...c, x: -c.x, rotation: -c.rotation, flip_x: !c.flip_x }
    : { ...c, y: -c.y, rotation: -c.rotation, flip_y: !c.flip_y }
}

function fill() {
  crop.value = { ...crop.value, x: 0, y: 0, zoom: fillZoom(natural.value, unit.value, crop.value.rotation) }
}

function fit() {
  crop.value = { ...crop.value, x: 0, y: 0, zoom: fitZoom(natural.value, unit.value, crop.value.rotation) }
}

function pointFrom(event: { clientX: number, clientY: number }) {
  const box = stage.value!.getBoundingClientRect()
  return { x: event.clientX - box.left - box.width / 2, y: event.clientY - box.top - box.height / 2 }
}

let drag: { x: number, y: number, crop: Crop } | null = null

function pointerDown(event: PointerEvent) {
  if (event.button !== 0 || status.value !== 'ready') {
    return
  }
  stage.value?.setPointerCapture(event.pointerId)
  drag = { x: event.clientX, y: event.clientY, crop: { ...crop.value } }
  moving.value = true
}

function pointerMove(event: PointerEvent) {
  if (!drag) {
    return
  }
  const { width, height } = frame.value
  let x = drag.crop.x * width + event.clientX - drag.x
  let y = drag.crop.y * height + event.clientY - drag.y
  snapped.value = { x: Math.abs(x) < SNAP, y: Math.abs(y) < SNAP }
  x = snapped.value.x ? 0 : x
  y = snapped.value.y ? 0 : y
  crop.value = { ...crop.value, x: round(x / width, 1000), y: round(y / height, 1000) }
}

function pointerUp() {
  drag = null
  moving.value = false
  snapped.value = { x: false, y: false }
}

function wheel(event: WheelEvent) {
  const speed = event.ctrlKey ? 0.01 : 0.002
  zoomTo(crop.value.zoom * Math.exp(-event.deltaY * speed), pointFrom(event))
}

let gestureStart = 1

function gestureBegin(event: Event) {
  event.preventDefault()
  gestureStart = crop.value.zoom
}

/** Trackpad pinch in WebKit, which does not send it as a wheel event. */
function gestureChange(event: Event) {
  event.preventDefault()
  const gesture = event as Event & { scale: number, clientX: number, clientY: number }
  zoomTo(gestureStart * gesture.scale, pointFrom(gesture))
}

function keydown(event: KeyboardEvent) {
  const step = event.shiftKey ? 0.1 : 0.01
  const c = crop.value
  switch (event.key) {
    case 'ArrowLeft': crop.value = { ...c, x: round(c.x - step, 1000) }; break
    case 'ArrowRight': crop.value = { ...c, x: round(c.x + step, 1000) }; break
    case 'ArrowUp': crop.value = { ...c, y: round(c.y - step, 1000) }; break
    case 'ArrowDown': crop.value = { ...c, y: round(c.y + step, 1000) }; break
    case '+':
    case '=': zoomTo(c.zoom * ZOOM_STEP); break
    case '-': zoomTo(c.zoom / ZOOM_STEP); break
    case 'r':
    case 'R': setRotation(c.rotation + (event.shiftKey ? -90 : 90)); break
    case 'Enter': apply(); break
    default: return
  }
  event.preventDefault()
}

async function apply() {
  if (!source.value || status.value !== 'ready') {
    return
  }
  status.value = 'saving'
  try {
    const blob = await bake(source.value, crop.value, output.value)
    const asset = await upload(blob, blob.type)
    image.value = { ...image.value, asset, crop: { ...crop.value }, fit: 'contain' }
    open.value = false
  }
  catch {
    status.value = 'ready'
    failed.value = true
  }
}

function round(value: number, precision: number) {
  return Math.round(value * precision) / precision
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="gap-4 sm:max-w-3xl">
      <DialogHeader>
        <DialogTitle>{{ t('editor.imageEditor.title') }}</DialogTitle>
        <DialogDescription>
          {{ screensaver ? t('editor.imageEditor.frameScreen') : t('editor.imageEditor.frameKey') }}
          {{ t('editor.imageEditor.originalKept') }}
        </DialogDescription>
      </DialogHeader>

      <div
        ref="stage"
        tabindex="0"
        role="application"
        :aria-label="t('editor.imageEditor.stage')"
        class="relative h-[min(52vh,420px)] touch-none overflow-hidden rounded-lg bg-[repeating-conic-gradient(#202020_0_25%,#171717_0_50%)] bg-size-[20px_20px] outline-none select-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="moving ? 'cursor-grabbing' : 'cursor-grab'"
        @pointerdown="pointerDown"
        @pointermove="pointerMove"
        @pointerup="pointerUp"
        @pointercancel="pointerUp"
        @wheel.prevent="wheel"
        @gesturestart="gestureBegin"
        @gesturechange="gestureChange"
        @dblclick="fill"
        @keydown="keydown"
      >
        <div class="absolute" :style="[frameStyle, { background: props.background }]" />
        <div class="pointer-events-none absolute" :style="frameStyle">
          <img
            v-if="shownUrl && status !== 'loading'"
            :src="shownUrl"
            alt=""
            draggable="false"
            class="absolute top-0 left-0 max-w-none origin-top-left"
            :style="imageStyle"
          >
        </div>
        <div
          class="pointer-events-none absolute shadow-[0_0_0_9999px_rgb(0_0_0/0.62)] ring-1 ring-white/70"
          :style="frameStyle"
        >
          <div v-for="(gap, i) in gapStyles" :key="i" class="absolute bg-black/45" :style="gap" />
          <template v-if="moving">
            <div class="absolute inset-y-0 left-1/3 w-px bg-white/25" />
            <div class="absolute inset-y-0 left-2/3 w-px bg-white/25" />
            <div class="absolute inset-x-0 top-1/3 h-px bg-white/25" />
            <div class="absolute inset-x-0 top-2/3 h-px bg-white/25" />
            <div v-if="snapped.x" class="absolute inset-y-0 left-1/2 w-px bg-primary" />
            <div v-if="snapped.y" class="absolute inset-x-0 top-1/2 h-px bg-primary" />
          </template>
        </div>
        <div v-if="status === 'loading'" class="absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">
          <Loader2 class="mr-2 size-4 animate-spin" /> {{ t('editor.imageEditor.loading') }}
        </div>
        <div class="pointer-events-none absolute right-3 bottom-2 left-3 flex justify-between text-[11px] text-white/55">
          <span>{{ t('editor.imageEditor.hint') }}</span>
          <span v-if="screensaver">{{ t('editor.imageEditor.gapsScreen') }}</span>
          <span v-else-if="gaps && status === 'ready'">{{ t('editor.imageEditor.gapsKey') }}</span>
        </div>
      </div>

      <div class="grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-3 text-xs">
        <span class="text-muted-foreground">{{ t('editor.imageEditor.zoom') }}</span>
        <div class="flex items-center gap-2">
          <Button variant="ghost" size="icon-sm" :aria-label="t('editor.imageEditor.zoomOut')" @click="zoomTo(crop.zoom / ZOOM_STEP)">
            <Minus />
          </Button>
          <Slider v-model="zoomSlider" :min="zoomRange.min" :max="zoomRange.max" :step="0.01" class="flex-1" :aria-label="t('editor.imageEditor.zoom')" />
          <Button variant="ghost" size="icon-sm" :aria-label="t('editor.imageEditor.zoomIn')" @click="zoomTo(crop.zoom * ZOOM_STEP)">
            <Plus />
          </Button>
        </div>
        <span class="w-12 text-right tabular-nums">{{ Math.round(crop.zoom * 100) }} %</span>

        <span class="text-muted-foreground">{{ t('editor.imageEditor.straighten') }}</span>
        <Slider v-model="straighten" :min="-45" :max="45" :step="0.5" class="px-9" :aria-label="t('editor.imageEditor.straighten')" />
        <span class="w-12 text-right tabular-nums">{{ straighten[0] }}°</span>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <div class="flex gap-1">
          <Button variant="outline" size="sm" :aria-label="t('editor.imageEditor.rotateLeft')" :title="t('editor.imageEditor.rotateLeftTitle')" @click="setRotation(crop.rotation - 90)">
            <RotateCcw />
          </Button>
          <Button variant="outline" size="sm" :aria-label="t('editor.imageEditor.rotateRight')" :title="t('editor.imageEditor.rotateRightTitle')" @click="setRotation(crop.rotation + 90)">
            <RotateCw />
          </Button>
          <Button variant="outline" size="sm" :aria-label="t('editor.imageEditor.flipX')" :title="t('editor.imageEditor.flipX')" @click="flip('x')">
            <FlipHorizontal2 />
          </Button>
          <Button variant="outline" size="sm" :aria-label="t('editor.imageEditor.flipY')" :title="t('editor.imageEditor.flipY')" @click="flip('y')">
            <FlipVertical2 />
          </Button>
        </div>
        <div class="flex gap-1">
          <Button variant="outline" size="sm" :title="screensaver ? t('editor.imageEditor.fillScreen') : t('editor.imageEditor.fillKey')" @click="fill">
            <Maximize /> {{ t('editor.imageEditor.fill') }}
          </Button>
          <Button variant="outline" size="sm" :title="t('editor.imageEditor.fitTitle')" @click="fit">
            <Minimize /> {{ t('editor.imageEditor.fit') }}
          </Button>
        </div>
        <Button variant="ghost" size="sm" @click="crop = identityCrop()">
          <Undo2 /> {{ t('editor.imageEditor.reset') }}
        </Button>
      </div>

      <DialogFooter class="items-center sm:justify-between">
        <p class="text-xs text-muted-foreground">
          <span v-if="status === 'error'" class="text-destructive">{{ t('editor.imageEditor.loadError') }}</span>
          <span v-else-if="failed" class="text-destructive">{{ t('editor.imageEditor.saveError') }}</span>
          <template v-else>
            {{ t(screensaver ? 'editor.imageEditor.copyScreen' : 'editor.imageEditor.copyKey', { width: output.width, height: output.height }) }}
          </template>
        </p>
        <div class="flex gap-2">
          <Button variant="outline" @click="open = false">
            {{ t('editor.imageEditor.cancel') }}
          </Button>
          <Button :disabled="status !== 'ready'" @click="apply">
            <Loader2 v-if="status === 'saving'" class="animate-spin" /> {{ t('editor.imageEditor.apply') }}
          </Button>
        </div>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
