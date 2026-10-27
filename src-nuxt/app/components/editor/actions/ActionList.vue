<script setup lang="ts">
import { CircleHelp, GripVertical, Plus, Puzzle, Trash2 } from '@lucide/vue'
import type { CatalogEntry } from '~/lib/actions/catalog'
import { entryFor } from '~/lib/actions/catalog'
import { KEY_DRAFT } from '~/lib/actions/key-draft'
import { KEY_SETUP } from '~/lib/actions/setup'
import { isDefaultKeyText } from '~/lib/new-key'
import type { Action, Gesture } from '~/types/engine'

const actions = defineModel<Action[]>({ required: true })
/** A key's double and long press lists; with them, each action says when it runs. */
const double = defineModel<Action[]>('double')
const long = defineModel<Action[]>('long')
const props = defineProps<{
  /** The page of the key, which its lines cannot lead to. */
  page: string
  /** The picks of a random action rather than a key's own list. */
  nested?: boolean
  /** A gesture or a second state, which cannot draw a line on the canvas. */
  noLines?: boolean
}>()

const { t } = useI18n()
const { pages } = useWorkspace()
const { snapshot } = useEngine()
const picking = ref(false)
const keySetup = inject(KEY_SETUP, null)

const otherPages = computed(() => pages.value.filter(p => p.id !== props.page))
const profiles = computed(() => snapshot.value?.profiles ?? [])
const navigates = computed(() => actions.value.some(a => entryFor(a)?.navigates))

function actionName(action: Action) {
  return entryFor(action)?.name ?? (action.type === 'plugin' ? t('actions.list.unknownPlugin') : action.type)
}

const keyDraft = inject(KEY_DRAFT, null)

function unavailable(entry: CatalogEntry) {
  if (entry.plugin) {
    if (props.nested || props.noLines) {
      return t('actions.list.unavailable.pluginOnlyPress')
    }
    if (keyDraft?.value?.second) {
      return t('actions.list.unavailable.pluginTwoStates')
    }
    const lists = [actions.value, double.value ?? [], long.value ?? []]
    if (lists.some(list => list.some(a => a.type === 'plugin'))) {
      return t('actions.list.unavailable.pluginTaken')
    }
  }
  if (entry.setup) {
    return keySetup ? keySetup.unavailable(entry.setup) : t('actions.list.unavailable.setUpFromKey')
  }
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

/** The action just picked, which opens its dialog if it is set up in one. */
const justAdded = shallowRef<Action | null>(null)
const JUST_ADDED_MS = 1000

function isJustAdded(action: Action) {
  return justAdded.value !== null && toRaw(action) === justAdded.value
}

function add(entry: CatalogEntry) {
  if (entry.setup) {
    keySetup?.apply(entry.setup)
    return
  }
  const action = entry.create?.({
    otherPages: otherPages.value.map(p => p.id),
    profiles: profiles.value.filter(p => p.id !== snapshot.value?.active_profile).map(p => p.id),
  })
  if (!action) {
    return
  }
  // The plugin draws the key: the text of a new key would only cover it.
  if (entry.plugin && keyDraft?.value && isDefaultKeyText(keyDraft.value.text?.content)) {
    keyDraft.value.text = null
  }
  justAdded.value = action
  setTimeout(() => {
    if (justAdded.value === action) {
      justAdded.value = null
    }
  }, JUST_ADDED_MS)
  // What moves the dock goes first unless the user moves it.
  if (entry.navigates) {
    actions.value.unshift(action)
  }
  else {
    actions.value.push(action)
  }
}

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

/** Moves an action to another "when", at the end of its group. */
function setWhen(from: Gesture, index: number, to: unknown) {
  if (typeof to !== 'string' || to === from) {
    return
  }
  const [action] = listOf(from).splice(index, 1)
  listOf(to as Gesture).push(action!)
}

const sortable = useSortable((gesture, from, to) => moveItem(listOf(gesture as Gesture), from, to))

</script>

<template>
  <div class="flex flex-col gap-2">
    <p v-if="!actions.length && !grouped" class="text-xs text-muted-foreground">
      {{ t(props.nested ? 'actions.list.emptyRandom' : 'actions.list.empty') }}
    </p>
    <section v-for="group in groups" :key="group.gesture" class="flex flex-col gap-2" :data-when="group.gesture">
      <p v-if="grouped" class="mt-1 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
        <WhenIcon :gesture="group.gesture" />
        {{ t(`${group.messages}.heading`) }}
      </p>
      <p v-if="grouped && !group.list.length" class="text-xs text-muted-foreground">
        {{ t('actions.list.groupEmpty') }}
      </p>
      <ol class="flex flex-col gap-2" data-sort-list>
        <li
          v-for="(action, i) in group.list"
          :key="i"
          :style="sortable.itemStyle(i, group.gesture)"
          class="flex flex-col gap-2 rounded-lg border p-2.5"
          :class="sortable.isDragging(i, group.gesture) ? 'relative z-10 border-sky-500 bg-neutral-900 shadow-lg shadow-black/40' : ''"
          data-sort-item
          :aria-label="t('actions.list.actionLabel', { n: i + 1, name: actionName(action) })"
        >
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="-ml-1 flex cursor-grab touch-none items-center rounded py-1 text-muted-foreground hover:text-foreground active:cursor-grabbing"
              :aria-label="t('actions.list.dragLabel', { n: i + 1 })"
              :title="t('actions.list.dragTitle')"
              data-sort-handle
              @pointerdown.prevent="sortable.press($event, i, group.gesture)"
            >
              <GripVertical class="size-3.5" />
              <span class="w-3 text-center text-xs tabular-nums">{{ i + 1 }}</span>
            </button>
            <img v-if="entryFor(action)?.image" :src="entryFor(action)!.image" alt="" class="size-7 shrink-0 rounded-md bg-neutral-800 object-cover">
            <div v-else class="flex size-7 shrink-0 items-center justify-center rounded-md bg-sky-500/15 text-sky-400">
              <component :is="entryFor(action)?.icon ?? (action.type === 'plugin' ? Puzzle : CircleHelp)" class="size-3.5" />
            </div>
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm font-medium">
                {{ actionName(action) }}
              </p>
              <p class="truncate text-[11px] text-muted-foreground">
                {{ entryFor(action)?.description }}
              </p>
            </div>
            <Button variant="ghost" size="icon-sm" :aria-label="t('actions.list.remove')" @click="group.list.splice(i, 1)">
              <Trash2 />
            </Button>
          </div>
          <div v-if="gestures" class="flex items-center gap-2 text-xs text-muted-foreground">
            {{ t('actions.list.when') }}
            <Select :model-value="group.gesture" @update:model-value="setWhen(group.gesture, i, $event)">
              <SelectTrigger size="sm" class="h-7 flex-1" :aria-label="t('actions.list.when')">
                <WhenIcon :gesture="group.gesture" class="text-muted-foreground" />
                <SelectValue class="flex-1 text-left" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem
                  v-for="w in WHEN"
                  :key="w.gesture"
                  :value="w.gesture"
                  :disabled="w.gesture !== 'press' && (action.type === 'open_page' || action.type === 'plugin')"
                >
                  <WhenIcon :gesture="w.gesture" />
                  {{ t(`${w.messages}.label`) }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          <ActionDetails v-model="group.list[i]!" :page="props.page" :other-pages="otherPages" :open-on-mount="isJustAdded(action)" />
        </li>
      </ol>
    </section>
    <Button variant="outline" size="sm" @click="picking = true">
      <Plus /> {{ t(props.nested ? 'actions.list.addRandom' : 'actions.list.add') }}
    </Button>
    <ActionPicker v-model:open="picking" :nested="props.nested" :unavailable="unavailable" @pick="add" />
  </div>
</template>
