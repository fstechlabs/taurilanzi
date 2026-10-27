<script setup lang="ts">
import { useLocalStorage } from '@vueuse/core'
import type { Hsv } from '~/lib/color'
import { hexToHsv, hsvToHex, normalizeHex } from '~/lib/color'

const color = defineModel<string>({ required: true })
const props = defineProps<{
  /** What the color is for, for screen readers and the title of the panel. */
  label: string
  /** Only the swatch, for tight spots. */
  compact?: boolean
}>()

const { t } = useI18n()

const RECENT_MAX = 8
const PALETTE = [
  ['#000000', '#18181b', '#3f3f46', '#71717a', '#a1a1aa', '#d4d4d8', '#f4f4f5', '#ffffff'],
  ['#ef4444', '#f97316', '#f59e0b', '#eab308', '#84cc16', '#22c55e', '#10b981', '#14b8a6'],
  ['#06b6d4', '#0ea5e9', '#3b82f6', '#6366f1', '#8b5cf6', '#a855f7', '#d946ef', '#ec4899'],
  ['#7f1d1d', '#7c2d12', '#78350f', '#365314', '#14532d', '#164e63', '#1e3a8a', '#4c1d95'],
]

const open = ref(false)
const recent = useLocalStorage<string[]>('recent-colors', [])
/** Kept apart from the hex so the hue survives going through black, white or grey. */
const hsv = ref<Hsv>(hexToHsv(color.value))
const hexText = ref(color.value)
let openedWith = color.value

watch(color, (value) => {
  if (hsvToHex(hsv.value) !== value) {
    hsv.value = hexToHsv(value)
  }
  hexText.value = value
})

watch(open, (isOpen) => {
  if (isOpen) {
    openedWith = color.value
    return
  }
  if (color.value !== openedWith) {
    recent.value = [color.value, ...recent.value.filter(c => c !== color.value)].slice(0, RECENT_MAX)
  }
})

function setHsv(next: Hsv) {
  hsv.value = next
  color.value = hsvToHex(next)
}

function pick(hex: string) {
  hsv.value = hexToHsv(hex)
  color.value = hex
}

function typed() {
  const hex = normalizeHex(hexText.value)
  if (hex) {
    pick(hex)
  }
  else {
    hexText.value = color.value
  }
}

/** Follows the pointer inside an area, also when it leaves it while dragging. */
function track(event: PointerEvent, apply: (x: number, y: number) => void) {
  const area = event.currentTarget as HTMLElement
  area.setPointerCapture(event.pointerId)
  const update = (e: PointerEvent) => {
    const box = area.getBoundingClientRect()
    apply(
      Math.min(1, Math.max(0, (e.clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (e.clientY - box.top) / box.height)),
    )
  }
  update(event)
  const stop = () => {
    area.removeEventListener('pointermove', update)
    area.removeEventListener('pointerup', stop)
  }
  area.addEventListener('pointermove', update)
  area.addEventListener('pointerup', stop)
}

function onSquare(event: PointerEvent) {
  track(event, (x, y) => setHsv({ ...hsv.value, s: x, v: 1 - y }))
}

function onHue(event: PointerEvent) {
  track(event, x => setHsv({ ...hsv.value, h: x * 360 }))
}

/** Arrow keys nudge the color, for those who prefer the keyboard. */
function nudge(event: KeyboardEvent, axis: 'square' | 'hue') {
  const step = event.shiftKey ? 0.1 : 0.01
  const { h, s, v } = hsv.value
  const moves: Record<string, Hsv> = axis === 'square'
    ? {
        ArrowLeft: { h, s: Math.max(0, s - step), v },
        ArrowRight: { h, s: Math.min(1, s + step), v },
        ArrowUp: { h, s, v: Math.min(1, v + step) },
        ArrowDown: { h, s, v: Math.max(0, v - step) },
      }
    : {
        ArrowLeft: { h: Math.max(0, h - step * 360), s, v },
        ArrowRight: { h: Math.min(360, h + step * 360), s, v },
      }
  const next = moves[event.key]
  if (next) {
    event.preventDefault()
    setHsv(next)
  }
}

const hueColor = computed(() => `hsl(${hsv.value.h} 100% 50%)`)
</script>

<template>
  <Popover v-model:open="open">
    <PopoverTrigger as-child>
      <button
        type="button"
        class="flex h-8 items-center gap-2 rounded-md border bg-transparent pr-2.5 pl-1 text-xs transition-colors hover:bg-neutral-800/60"
        :class="props.compact ? 'w-10 justify-center pr-1' : 'min-w-0 flex-1'"
        :aria-label="props.label"
        :title="props.label"
        data-color-picker
      >
        <span class="size-6 shrink-0 rounded-[5px] ring-1 ring-white/15 ring-inset" :style="{ background: color }" />
        <span v-if="!props.compact" class="font-mono uppercase text-muted-foreground">{{ color }}</span>
      </button>
    </PopoverTrigger>
    <PopoverContent align="start" class="w-64 p-3">
      <p class="mb-2 text-xs font-medium">
        {{ props.label }}
      </p>

      <div
        class="relative h-36 cursor-crosshair touch-none overflow-hidden rounded-md select-none"
        :style="{ background: hueColor }"
        role="slider"
        tabindex="0"
        :aria-label="t('common.colorPicker.saturation')"
        :aria-valuetext="color"
        @pointerdown.prevent="onSquare"
        @keydown="nudge($event, 'square')"
      >
        <div class="absolute inset-0 bg-linear-to-r from-white to-transparent" />
        <div class="absolute inset-0 bg-linear-to-t from-black to-transparent" />
        <span
          class="pointer-events-none absolute size-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0/0.4),0_2px_6px_rgb(0_0_0/0.5)]"
          :style="{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, background: color }"
        />
      </div>

      <div
        class="relative mt-3 h-3 cursor-pointer touch-none rounded-full select-none"
        style="background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)"
        role="slider"
        tabindex="0"
        :aria-label="t('common.colorPicker.hue')"
        :aria-valuenow="Math.round(hsv.h)"
        aria-valuemin="0"
        aria-valuemax="360"
        @pointerdown.prevent="onHue"
        @keydown="nudge($event, 'hue')"
      >
        <span
          class="pointer-events-none absolute top-1/2 size-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0/0.4)]"
          :style="{ left: `${(hsv.h / 360) * 100}%`, background: hueColor }"
        />
      </div>

      <div class="mt-3 flex items-center gap-2">
        <span class="size-8 shrink-0 rounded-md ring-1 ring-white/15 ring-inset" :style="{ background: color }" />
        <Input
          v-model="hexText"
          class="h-8 font-mono text-xs uppercase"
          :aria-label="t('common.colorPicker.hex')"
          spellcheck="false"
          @keydown.enter.prevent="typed"
          @blur="typed"
        />
      </div>

      <div class="mt-3 flex flex-col gap-1">
        <div v-for="(row, r) in PALETTE" :key="r" class="grid grid-cols-8 gap-1">
          <button
            v-for="swatch in row"
            :key="swatch"
            type="button"
            class="aspect-square rounded-[5px] ring-1 ring-white/10 ring-inset transition-transform hover:scale-110"
            :class="swatch === color ? 'outline-2 outline-offset-1 outline-sky-400' : ''"
            :style="{ background: swatch }"
            :aria-label="swatch"
            :data-swatch="swatch"
            @click="pick(swatch)"
          />
        </div>
      </div>

      <template v-if="recent.length">
        <p class="mt-3 mb-1 text-[11px] text-muted-foreground">
          {{ t('common.colorPicker.recent') }}
        </p>
        <div class="grid grid-cols-8 gap-1">
          <button
            v-for="swatch in recent"
            :key="swatch"
            type="button"
            class="aspect-square rounded-[5px] ring-1 ring-white/10 ring-inset transition-transform hover:scale-110"
            :style="{ background: swatch }"
            :aria-label="t('common.colorPicker.recentSwatch', { color: swatch })"
            @click="pick(swatch)"
          />
        </div>
      </template>
    </PopoverContent>
  </Popover>
</template>
