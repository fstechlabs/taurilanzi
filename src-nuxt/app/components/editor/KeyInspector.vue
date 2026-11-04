<script setup lang="ts">
import { Check, ChevronsUpDown, Play, Plus, Trash2 } from '@lucide/vue'
import { useLocalStorage } from '@vueuse/core'
import type { KeyRef } from '~/composables/useWorkspace'
import { appearanceFrom } from '~/composables/useKeyEditor'
import type { CatalogEntry } from '~/lib/actions/catalog'
import { KEY_DRAFT, KEY_TARGET } from '~/lib/actions/key-draft'
import { applyType, hasTypeSettings } from '~/lib/key-kinds'
import type { Gesture } from '~/types/engine'

const WIDE_KEY = 13
/** 30 % wider than the first inspector, which was too tight for the action editors. */
const DEFAULT_WIDTH = 500
const MIN_WIDTH = 340
const MAX_WIDTH = 900

const props = defineProps<{ target: KeyRef | null }>()
const { t } = useI18n()
const target = toRef(props, 'target')

const { stored, draft, state, editing, shownWidget, preview, dirty, invalid, status, error, create, clear, uploadImage, pickIcon, tryKey } = useKeyEditor(target)
const { pages } = useWorkspace()
const { snapshot, previewUrl } = useEngine()

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

const otherPages = computed(() => pages.value.filter(p => p.id !== props.target?.page))
const profiles = computed(() => snapshot.value?.profiles ?? [])
const configuringType = ref(false)
const choosingType = ref(false)
watch(() => props.target && `${props.target.page}:${props.target.index}`, () => {
  configuringType.value = false
  choosingType.value = false
})

/** A new key is of the type picked first. */
function choose(entry: CatalogEntry) {
  create()
  if (!draft.value) {
    return
  }
  applyType(draft.value, entry, {
    otherPages: otherPages.value.map(p => p.id),
    profiles: profiles.value.filter(p => p.id !== snapshot.value?.active_profile).map(p => p.id),
  })
  if (hasTypeSettings(draft.value)) {
    // Once the catalog has closed: two dialogs at once fight over the focus.
    setTimeout(() => {
      configuringType.value = true
    }, 250)
  }
}

function unavailableType(entry: CatalogEntry) {
  if (entry.id === 'open_page' && !otherPages.value.length) {
    return t('actions.list.unavailable.noOtherPage')
  }
  if (entry.id === 'switch_profile' && profiles.value.length < 2) {
    return t('actions.list.unavailable.oneProfile')
  }
  return null
}

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

provide(KEY_DRAFT, draft)
provide(KEY_TARGET, target)

/** For the state tabs: a small preview of each state. */
const thumbnails = computed(() => (draft.value?.second && props.target ? [draft.value, draft.value.second] : []))
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

    <div v-else-if="!draft" class="flex flex-col" data-choose-type>
      <div class="flex flex-col items-center gap-2 border-b p-5">
        <div class="rounded-xl bg-neutral-900" :class="wide ? 'aspect-[452/196] w-64' : 'aspect-square w-28'" />
        <p class="text-sm font-medium">
          {{ title }} · {{ pageName }}
        </p>
      </div>
      <section class="flex flex-col gap-3 border-b border-sky-950/70 bg-[#0d121a] px-4 pt-3 pb-4" data-section="type">
        <h2 class="text-[11px] font-semibold tracking-wider text-sky-200/70 uppercase">
          {{ t('editor.inspector.typeSection') }}
        </h2>
        <button
          type="button"
          class="flex items-center gap-3 rounded-lg border border-dashed border-sky-500/50 bg-sky-500/5 p-2.5 text-left transition-colors hover:border-sky-500 hover:bg-sky-500/10"
          data-change-type
          @click="choosingType = true"
        >
          <span class="flex size-9 shrink-0 items-center justify-center rounded-md bg-sky-500/15 text-sky-400">
            <Plus class="size-4.5" />
          </span>
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium">{{ t('editor.type.choose') }}</span>
            <span class="block text-xs text-muted-foreground">{{ t('editor.type.firstChoose') }}</span>
          </span>
          <ChevronsUpDown class="size-4 shrink-0 text-muted-foreground" />
        </button>
      </section>
      <ActionPicker v-model:open="choosingType" mode="type" :unavailable="unavailableType" @pick="choose" />
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
        </div>

        <section class="flex flex-col gap-3 border-b border-sky-950/70 bg-[#0d121a] px-4 pt-3 pb-4" data-section="type">
          <h2 class="text-[11px] font-semibold tracking-wider text-sky-200/70 uppercase">
            {{ t('editor.inspector.typeSection') }}
          </h2>
          <KeyTypeSection v-model="draft" v-model:configuring="configuringType" v-model:shown-widget="shownWidget" :page="props.target.page" />
        </section>

        <section class="flex flex-col gap-3 border-b border-violet-950/70 bg-[#110e18] px-4 pt-3 pb-4" data-section="actions">
          <h2 class="text-[11px] font-semibold tracking-wider text-violet-200/70 uppercase">
            {{ t('editor.inspector.actions') }}
          </h2>
          <template v-if="draft.second">
            <p class="text-[11px] text-muted-foreground">
              {{ t('editor.inspector.actionsHint') }}
            </p>
            <div v-for="(look, i) in [draft, draft.second]" :key="i" class="flex flex-col gap-2" :data-state-actions="i + 1">
              <p class="text-xs font-medium">
                {{ t('editor.inspector.state', { number: i + 1 }) }}
              </p>
              <ActionList
                v-model="look.actions"
                :page="props.target.page"
                :main="look.main"
                :no-lines="i === 1"
                :empty="t(look.main ? 'editor.inspector.noMoreActions' : 'actions.list.empty')"
              />
            </div>
          </template>
          <ActionList
            v-else
            v-model="draft.actions"
            :double="draft.doublePress"
            :long="draft.longPress"
            :page="props.target.page"
            :main="draft.main"
            :empty="t(draft.main ? 'editor.inspector.noMoreActions' : 'actions.list.empty')"
            @update:double="draft.doublePress = $event ?? []"
            @update:long="draft.longPress = $event ?? []"
          />
        </section>

        <section v-if="state" class="flex flex-col gap-4 border-b border-emerald-950/70 bg-[#0d1410] px-4 pt-3 pb-4" data-section="look">
          <div class="flex items-center justify-between gap-2">
            <h2 class="text-[11px] font-semibold tracking-wider text-emerald-200/70 uppercase">
              {{ t('editor.inspector.look') }}
            </h2>
            <div v-if="thumbnails.length" class="flex gap-1.5" role="tablist" :aria-label="t('editor.inspector.states')">
              <button
                v-for="(look, i) in thumbnails"
                :key="i"
                type="button"
                role="tab"
                class="flex items-center gap-1.5 rounded-md border px-1.5 py-0.5 text-xs transition-colors"
                :class="editing === i ? 'border-emerald-500/70 bg-emerald-500/10 text-foreground' : 'text-muted-foreground hover:text-foreground'"
                :aria-selected="editing === i"
                :aria-label="t('editor.inspector.state', { number: i + 1 })"
                @click="editing = i as 0 | 1"
              >
                <img :src="previewUrl(props.target.index, appearanceFrom(look), props.target.page)" alt="" class="size-5 rounded">
                {{ t('editor.inspector.state', { number: i + 1 }) }}
              </button>
            </div>
          </div>
          <BackgroundFields v-model="state" />
          <ImageFields v-model="state" :wide="wide" @upload="uploadImage" @pick-icon="pickIcon" />
          <TextFields v-model="state" />
        </section>
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
