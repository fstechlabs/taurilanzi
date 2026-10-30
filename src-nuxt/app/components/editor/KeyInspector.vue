<script setup lang="ts">
import { Check, ChevronDown, Play, Plus, Trash2 } from '@lucide/vue'
import { useLocalStorage } from '@vueuse/core'
import type { KeyRef } from '~/composables/useWorkspace'
import { appearanceFrom } from '~/composables/useKeyEditor'
import type { KeySetup } from '~/lib/actions/catalog'
import { KEY_SETUP } from '~/lib/actions/setup'
import { KEY_DRAFT, KEY_TARGET } from '~/lib/actions/key-draft'
import type { Action, Gesture } from '~/types/engine'

const WIDE_KEY = 13
/** 30 % wider than the first inspector, which was too tight for the action editors. */
const DEFAULT_WIDTH = 500
const MIN_WIDTH = 340
const MAX_WIDTH = 900
const INDICATOR = '{page_number}/{page_count}'

const props = defineProps<{ target: KeyRef | null }>()
const { t } = useI18n()
const target = toRef(props, 'target')

const { stored, draft, state, editing, shownWidget, preview, dirty, invalid, status, error, create, clear, uploadImage, pickIcon, tryKey } = useKeyEditor(target)
const { pages } = useWorkspace()

/** Dragging the left edge resizes the inspector; the width is kept for next time. */
const width = useLocalStorage('inspector-width', DEFAULT_WIDTH)
let resizing: { x: number, width: number } | null = null

function startResize(event: PointerEvent) {
  resizing = { x: event.clientX, width: width.value }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}

function resize(event: PointerEvent) {
  if (resizing) {
    width.value = Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, resizing.width + resizing.x - event.clientX)))
  }
}

function stopResize() {
  resizing = null
}

const wide = computed(() => props.target?.index === WIDE_KEY)
const title = computed(() => (wide.value ? t('editor.inspector.wideKey') : t('editor.inspector.key', { number: (props.target?.index ?? 0) + 1 })))
const pageName = computed(() => pages.value.find(p => p.id === props.target?.page)?.name ?? '')

const hasGestures = computed(() => Boolean(draft.value?.doublePress.length || draft.value?.longPress.length))
/** The section the catalog opens when it gives the key two states. */
const statesOpen = ref(false)
watch(() => props.target && `${props.target.page}:${props.target.index}`, () => {
  statesOpen.value = Boolean(draft.value?.second)
}, { immediate: true })

const twoStates = computed({
  get: () => Boolean(draft.value?.second),
  set: (on) => {
    if (!draft.value) {
      return
    }
    if (on) {
      // The second state starts as a copy of the first, to change just what differs.
      const { background, from, to, angle, image, text, untouched, widget, moreWidgets } = draft.value
      draft.value.second = JSON.parse(JSON.stringify({ background, from, to, angle, image, text, untouched, actions: [], widget, moreWidgets }))
      editing.value = 1
    }
    else {
      draft.value.second = null
    }
  },
})

const GESTURES: { gesture: Gesture, label: string }[] = [
  { gesture: 'press', label: 'editor.gestures.press' },
  { gesture: 'double_press', label: 'editor.gestures.doublePress' },
  { gesture: 'long_press', label: 'editor.gestures.longPress' },
]

const canTry = computed(() => {
  const key = stored.value
  const rotates = key && key.appearance.layers.filter(layer => layer.type === 'widget').length > 1
  return Boolean(key && (key.actions.length || key.double_press.length || key.long_press.length || key.second_state || rotates))
})

provide(KEY_SETUP, {
  unavailable(setup: KeySetup) {
    if ((setup === 'two_states' || setup === 'switch_hotkey') && hasGestures.value) {
      return t('editor.inspector.twoStatesUnavailable')
    }
    return null
  },
  apply(setup: KeySetup) {
    const key = draft.value
    if (!key) {
      return
    }
    if (setup === 'page_indicator') {
      const look = state.value ?? key
      look.text = look.text
        ? { ...look.text, content: INDICATOR }
        : { content: INDICATOR, fontFamily: 'Inter', fontWeight: 700, size: 40, color: '#ffffff', position: 'middle' }
      return
    }
    if (!key.second) {
      twoStates.value = true
    }
    statesOpen.value = true
    if (setup === 'switch_hotkey') {
      const hotkey = (): Action => ({ type: 'hotkey', keys: { key: '', command: false, shift: false, option: false, control: false } })
      for (const look of [key, key.second!]) {
        if (!look.actions.some(a => a.type === 'hotkey')) {
          look.actions.push(hotkey())
        }
      }
    }
  },
})

provide(KEY_DRAFT, draft)
provide(KEY_TARGET, target)

/** For the state tabs: a small preview of each state. */
const thumbnails = computed(() => (draft.value?.second && props.target ? [draft.value, draft.value.second] : []))
const { previewUrl } = useEngine()
</script>

<template>
  <aside class="relative flex shrink-0 flex-col border-l bg-neutral-900/40" :style="{ width: `${width}px` }">
    <div
      class="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize transition-colors hover:bg-sky-500/40"
      role="separator"
      aria-orientation="vertical"
      :aria-label="t('editor.inspector.resize')"
      data-inspector-resize
      @pointerdown.prevent="startResize"
      @pointermove="resize"
      @pointerup="stopResize"
      @pointercancel="stopResize"
      @dblclick="width = DEFAULT_WIDTH"
    />
    <div v-if="!props.target" class="flex flex-1 items-center justify-center p-8 text-center text-sm text-muted-foreground">
      {{ t('editor.inspector.noKey') }}
    </div>

    <div v-else-if="!draft" class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
      <p class="text-sm font-medium">
        {{ title }}
      </p>
      <p class="text-xs text-muted-foreground">
        {{ pageName }}
      </p>
      <p class="text-sm text-muted-foreground">
        {{ t('editor.inspector.empty') }}
      </p>
      <Button size="sm" @click="create">
        <Plus /> {{ t('editor.inspector.setUp') }}
      </Button>
    </div>

    <template v-else>
      <ScrollArea class="min-h-0 flex-1">
        <div class="flex flex-col items-center gap-2 border-b p-5">
          <div :class="wide ? 'w-64' : 'w-28'">
            <img v-if="preview" :src="preview" alt="" class="w-full" :class="wide ? 'rounded-[4%/9%]' : 'rounded-[9%]'" data-key-preview>
            <div v-else class="w-full rounded-xl bg-neutral-900" :class="wide ? 'aspect-[452/196]' : 'aspect-square'" />
          </div>
          <p class="text-sm font-medium">
            {{ title }} · {{ pageName }}
          </p>
          <p class="text-xs text-muted-foreground">
            {{ t('editor.inspector.preview', { size: wide ? '452 × 196' : '196 × 196' }) }}
          </p>
          <div v-if="thumbnails.length" class="mt-1 flex gap-2" role="tablist" :aria-label="t('editor.inspector.states')">
            <button
              v-for="(look, i) in thumbnails"
              :key="i"
              type="button"
              role="tab"
              class="flex items-center gap-2 rounded-md border px-2 py-1 text-xs transition-colors"
              :class="editing === i ? 'border-sky-500 bg-sky-500/10 text-foreground' : 'text-muted-foreground hover:text-foreground'"
              :aria-selected="editing === i"
              :aria-label="t('editor.inspector.state', { number: i + 1 })"
              @click="editing = i as 0 | 1"
            >
              <img :src="previewUrl(props.target.index, appearanceFrom(look), props.target.page)" alt="" class="size-6 rounded">
              {{ t('editor.inspector.state', { number: i + 1 }) }}
            </button>
          </div>
        </div>

        <Collapsible v-if="state" default-open class="border-b">
          <CollapsibleTrigger class="flex w-full items-center justify-between px-4 py-3 text-sm font-medium">
            {{ draft.second ? t('editor.inspector.lookOfState', { number: editing + 1 }) : t('editor.inspector.look') }} <ChevronDown class="size-4 text-muted-foreground" />
          </CollapsibleTrigger>
          <CollapsibleContent class="flex flex-col gap-5 px-4 pb-4">
            <BackgroundFields v-model="state" />
            <ImageFields v-model="state" :wide="wide" @upload="uploadImage" @pick-icon="pickIcon" />
            <TextFields v-model="state" />
          </CollapsibleContent>
        </Collapsible>

        <Collapsible v-if="state && (wide || state.widget)" default-open class="border-b">
          <CollapsibleTrigger class="flex w-full items-center justify-between px-4 py-3 text-sm font-medium">
            {{ draft.second ? t('editor.inspector.liveOfState', { number: editing + 1 }) : t('editor.inspector.live') }}
            <ChevronDown class="size-4 text-muted-foreground" />
          </CollapsibleTrigger>
          <CollapsibleContent class="px-4 pb-4">
            <WidgetFields v-model="state" v-model:shown="shownWidget" />
          </CollapsibleContent>
        </Collapsible>

        <Collapsible v-if="state" default-open class="border-b">
          <CollapsibleTrigger class="flex w-full items-center justify-between px-4 py-3 text-sm font-medium">
            {{ draft.second ? t('editor.inspector.actionsOfState', { number: editing + 1 }) : t('editor.inspector.actions') }}
            <ChevronDown class="size-4 text-muted-foreground" />
          </CollapsibleTrigger>
          <CollapsibleContent class="px-4 pb-4">
            <p v-if="draft.second" class="mb-2 text-[11px] text-muted-foreground">
              {{ t('editor.inspector.actionsHint') }}
            </p>
            <!-- One list for both cases, so an action that gives the key two states from its dialog
                 keeps that dialog open. -->
            <ActionList
              :key="editing"
              v-model="state.actions"
              :double="draft.second ? undefined : draft.doublePress"
              :long="draft.second ? undefined : draft.longPress"
              :page="props.target.page"
              :no-lines="editing === 1"
              @update:double="draft.doublePress = $event ?? []"
              @update:long="draft.longPress = $event ?? []"
            />
          </CollapsibleContent>
        </Collapsible>

        <Collapsible v-model:open="statesOpen" class="border-b">
          <CollapsibleTrigger class="flex w-full items-center justify-between px-4 py-3 text-sm font-medium">
            {{ t('editor.inspector.statesSection') }} <ChevronDown class="size-4 text-muted-foreground" />
          </CollapsibleTrigger>
          <CollapsibleContent class="flex flex-col gap-2 px-4 pb-4">
            <label class="flex items-center justify-between gap-2 text-sm">
              <span>{{ t('editor.inspector.twoStates') }} <span class="block text-[11px] text-muted-foreground">{{ t('editor.inspector.twoStatesHint') }}</span></span>
              <Switch v-model="twoStates" :disabled="hasGestures" :aria-label="t('editor.inspector.twoStates')" />
            </label>
            <p v-if="hasGestures" class="text-[11px] text-muted-foreground">
              {{ t('editor.inspector.twoStatesBlocked') }}
            </p>
          </CollapsibleContent>
        </Collapsible>
      </ScrollArea>

      <div class="flex flex-col gap-2 border-t p-3">
        <div class="flex items-center gap-2">
          <p class="flex min-w-0 flex-1 items-center gap-1.5 text-xs" :class="error || invalid ? 'text-destructive' : 'text-muted-foreground'" data-save-status aria-live="polite">
            <template v-if="error">
              {{ error }}
            </template>
            <template v-else-if="invalid">
              {{ t('editor.inspector.unsaved', { problem: invalid.charAt(0).toLowerCase() + invalid.slice(1) }) }}
            </template>
            <template v-else-if="status === 'saved'">
              <Check class="size-3.5" /> {{ t('editor.inspector.saved') }}
            </template>
            <template v-else>
              {{ t('editor.inspector.saving') }}
            </template>
          </p>
          <DropdownMenu v-if="stored && (stored.double_press.length || stored.long_press.length)">
            <DropdownMenuTrigger as-child>
              <Button variant="outline" size="icon" :aria-label="t('editor.inspector.tryKey')" :title="t('editor.inspector.tryGesture')">
                <Play />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" class="w-48">
              <DropdownMenuItem
                v-for="g in GESTURES"
                :key="g.gesture"
                :disabled="g.gesture === 'double_press' ? !stored.double_press.length : g.gesture === 'long_press' ? !stored.long_press.length : false"
                @select="tryKey(g.gesture)"
              >
                {{ t(g.label) }}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button
            v-else-if="stored"
            variant="outline"
            size="icon"
            :disabled="dirty || !canTry"
            :aria-label="t('editor.inspector.tryKey')"
            :title="dirty ? t('editor.inspector.tryWait') : t('editor.inspector.tryHint')"
            @click="tryKey()"
          >
            <Play />
          </Button>
          <Button v-if="stored" variant="outline" size="icon" :aria-label="t('editor.inspector.clearKey')" @click="clear">
            <Trash2 />
          </Button>
        </div>
      </div>
    </template>
  </aside>
</template>
