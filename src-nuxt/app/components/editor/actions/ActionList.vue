<script setup lang="ts">
import { ChevronRight, CircleHelp, GripVertical, Plus, Puzzle, Trash2 } from '@lucide/vue'
import { actionsError } from '~/composables/useKeyEditor'
import type { CatalogEntry } from '~/lib/actions/catalog'
import { entryFor } from '~/lib/actions/catalog'
import type { Action, Gesture } from '~/types/engine'

const actions = defineModel<Action[]>({ required: true })
/** A key's double and long press lists; with them, each action says when it runs. */
const double = defineModel<Action[]>('double')
const long = defineModel<Action[]>('long')
const props = defineProps<{
  /** The page of the key, which its lines cannot lead to. */
  page: string
  /** The actions of a key, or the picks of a random action. */
  mode?: 'action' | 'random'
  /** A gesture or a second state, which cannot draw a line on the canvas. */
  noLines?: boolean
  /** What the key is, run before the list: it counts when only one action may move the dock. */
  main?: Action | null
  /** What to say when the list is empty. */
  empty?: string
}>()

const { t } = useI18n()
const { pages } = useWorkspace()
const { snapshot } = useEngine()
const { summary } = useActionSummary()
const picking = ref(false)
const random = computed(() => props.mode === 'random')

const otherPages = computed(() => pages.value.filter(p => p.id !== props.page))
const profiles = computed(() => snapshot.value?.profiles ?? [])
const servers = computed(() => (snapshot.value?.integrations ?? []).map(server => server.id))
const navigates = computed(() => [props.main, ...actions.value].some(a => a && entryFor(a)?.navigates))

function actionName(action: Action) {
  return entryFor(action)?.name ?? (action.type === 'plugin' ? t('actions.list.unknownPlugin') : action.type)
}

function unavailable(entry: CatalogEntry) {
  if (entry.navigates && navigates.value) {
    return t('actions.list.unavailable.navigatesOnce')
  }
  if (entry.id === 'open_page' && props.noLines) {
    return t('actions.list.unavailable.linesOnlyPress')
  }
  if (entry.id === 'open_page' && !otherPages.value.length) {
    return t('actions.list.unavailable.noOtherPage')
  }
  if (entry.id === 'switch_profile' && profiles.value.length < 2) {
    return t('actions.list.unavailable.oneProfile')
  }
  return null
}

/** Actions that have nothing to set up open no dialog when added. */
const NOTHING_TO_SET: Action['type'][] = ['back', 'home', 'previous_page', 'next_page', 'stop_sounds']

// The gestures for "when" an action runs; `actions.list.gestures` has their labels, which also
// head their groups.
const WHEN: { gesture: Gesture, messages: string }[] = [
  { gesture: 'press', messages: 'actions.list.gestures.press' },
  { gesture: 'double_press', messages: 'actions.list.gestures.doublePress' },
  { gesture: 'long_press', messages: 'actions.list.gestures.longPress' },
]

const gestures = computed(() => Boolean(double.value && long.value))

function listOf(gesture: Gesture): Action[] {
  if (gesture === 'double_press') {
    return double.value!
  }
  if (gesture === 'long_press') {
    return long.value!
  }
  return actions.value
}

const groups = computed(() => WHEN
  .filter(w => w.gesture === 'press' || (gestures.value && listOf(w.gesture).length))
  .map(w => ({ ...w, list: listOf(w.gesture) })))
const grouped = computed(() => groups.value.length > 1)

/** The action whose dialog is open, by its list and place in it. */
const editing = ref<{ gesture: Gesture, index: number } | null>(null)
const dialogOpen = computed({
  get: () => editing.value !== null,
  set: (open) => {
    if (!open) {
      editing.value = null
    }
  },
})
const edited = computed({
  get: () => (editing.value ? listOf(editing.value.gesture)[editing.value.index] ?? null : null),
  set: (action) => {
    if (editing.value && action) {
      listOf(editing.value.gesture)[editing.value.index] = action
    }
  },
})

/** Long enough for the catalog to finish closing: two dialogs at once fight over the focus. */
const AFTER_CATALOG_MS = 250

function add(entry: CatalogEntry) {
  const action = entry.create?.({
    otherPages: otherPages.value.map(p => p.id),
    profiles: profiles.value.filter(p => p.id !== snapshot.value?.active_profile).map(p => p.id),
  })
  if (!action) {
    return
  }
  // What moves the dock goes first unless the user moves it.
  if (entry.navigates) {
    actions.value.unshift(action)
  }
  else {
    actions.value.push(action)
  }
  if (!NOTHING_TO_SET.includes(action.type)) {
    const index = entry.navigates ? 0 : actions.value.length - 1
    setTimeout(() => {
      editing.value = { gesture: 'press', index }
    }, AFTER_CATALOG_MS)
  }
}

/** Moves the action being edited to another "when", at the end of its group. */
function setWhen(to: unknown) {
  const from = editing.value
  if (!from || typeof to !== 'string' || to === from.gesture) {
    return
  }
  const [action] = listOf(from.gesture).splice(from.index, 1)
  const target = listOf(to as Gesture)
  target.push(action!)
  editing.value = { gesture: to as Gesture, index: target.length - 1 }
}

function remove(gesture: Gesture, index: number) {
  listOf(gesture).splice(index, 1)
}

function problem(action: Action) {
  return actionsError([action], servers.value)
}

const sortable = useSortable((gesture, from, to) => moveItem(listOf(gesture as Gesture), from, to))
</script>

<template>
  <div class="flex flex-col gap-2">
    <p v-if="!actions.length && !grouped" class="text-xs text-muted-foreground">
      {{ props.empty ?? t(random ? 'actions.list.emptyRandom' : 'actions.list.empty') }}
    </p>
    <section v-for="group in groups" :key="group.gesture" class="flex flex-col gap-1.5" :data-when="group.gesture">
      <p v-if="grouped" class="mt-1 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
        <WhenIcon :gesture="group.gesture" />
        {{ t(`${group.messages}.heading`) }}
      </p>
      <p v-if="grouped && !group.list.length" class="text-xs text-muted-foreground">
        {{ t('actions.list.groupEmpty') }}
      </p>
      <ol class="flex flex-col gap-1.5" data-sort-list>
        <li
          v-for="(action, i) in group.list"
          :key="i"
          :style="sortable.itemStyle(i, group.gesture)"
          class="group/action flex items-center gap-2 rounded-lg border bg-neutral-950/40 py-1.5 pr-1.5 pl-2 transition-colors hover:border-sky-500/50"
          :class="sortable.isDragging(i, group.gesture) ? 'relative z-10 border-sky-500 bg-neutral-900 shadow-lg shadow-black/40' : ''"
          data-sort-item
          :aria-label="t('actions.list.actionLabel', { n: i + 1, name: actionName(action) })"
        >
          <button
            type="button"
            class="-ml-1 flex cursor-grab touch-none items-center rounded py-1 text-muted-foreground hover:text-foreground active:cursor-grabbing"
            :aria-label="t('actions.list.dragLabel', { n: i + 1 })"
            :title="t('actions.list.dragTitle')"
            data-sort-handle
            @pointerdown.prevent="sortable.press($event, i, group.gesture)"
          >
            <GripVertical class="size-3.5" />
          </button>
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center gap-2 text-left"
            :aria-label="t('actions.details.configureLabel', { name: actionName(action) })"
            data-action-summary
            @click="editing = { gesture: group.gesture, index: i }"
          >
            <img v-if="entryFor(action)?.image" :src="entryFor(action)!.image" alt="" class="size-7 shrink-0 rounded-md bg-neutral-800 object-cover">
            <span v-else class="flex size-7 shrink-0 items-center justify-center rounded-md bg-sky-500/15 text-sky-400">
              <component :is="entryFor(action)?.icon ?? (action.type === 'plugin' ? Puzzle : CircleHelp)" class="size-3.5" />
            </span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-[11px] text-muted-foreground">{{ actionName(action) }}</span>
              <span class="block truncate text-sm" :class="problem(action) ? 'text-destructive' : ''" data-summary>{{ summary(action) }}</span>
            </span>
            <ChevronRight class="size-4 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover/action:opacity-100" />
          </button>
          <Button variant="ghost" size="icon-sm" :aria-label="t('actions.list.remove')" @click="remove(group.gesture, i)">
            <Trash2 />
          </Button>
        </li>
      </ol>
    </section>
    <Button variant="outline" size="sm" @click="picking = true">
      <Plus /> {{ t(random ? 'actions.list.addRandom' : 'actions.list.add') }}
    </Button>
    <ActionPicker v-model:open="picking" :mode="random ? 'random' : 'action'" :unavailable="unavailable" @pick="add" />
    <ActionDialog v-if="edited" v-model:open="dialogOpen" v-model:action="edited" :page="props.page" :other-pages="otherPages">
      <div v-if="gestures && editing" class="flex items-center gap-2 text-xs text-muted-foreground">
        {{ t('actions.list.when') }}
        <Select :model-value="editing.gesture" @update:model-value="setWhen">
          <SelectTrigger size="sm" class="h-8 flex-1" :aria-label="t('actions.list.when')">
            <WhenIcon :gesture="editing.gesture" class="text-muted-foreground" />
            <SelectValue class="flex-1 text-left" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem
              v-for="w in WHEN"
              :key="w.gesture"
              :value="w.gesture"
              :disabled="w.gesture !== 'press' && edited.type === 'open_page'"
            >
              <WhenIcon :gesture="w.gesture" />
              {{ t(`${w.messages}.label`) }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
    </ActionDialog>
  </div>
</template>
