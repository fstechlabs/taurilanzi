<script setup lang="ts">
import { ChevronsUpDown, CircleHelp, Settings2 } from '@lucide/vue'
import type { KeyDraft } from '~/composables/useKeyEditor'
import { actionsError, widgetsOf } from '~/composables/useKeyEditor'
import type { CatalogEntry } from '~/lib/actions/catalog'
import { entryFor } from '~/lib/actions/catalog'
import { applyType, hasTypeSettings, typeEntry } from '~/lib/key-kinds'
import { widgetLabel } from '~/lib/widgets'
import type { Action } from '~/types/engine'

const draft = defineModel<KeyDraft>({ required: true })
/** The dialog of the type's settings, open. */
const configuring = defineModel<boolean>('configuring', { default: false })
const props = defineProps<{ page: string }>()

const { t } = useI18n()
const { pages } = useWorkspace()
const { snapshot } = useEngine()
const { summary } = useActionSummary()
const changing = ref(false)
/** Which of the widgets of the key the dialog edits. */
const shownWidget = defineModel<number>('shownWidget', { default: 0 })

const entry = computed(() => typeEntry(draft.value))
const otherPages = computed(() => pages.value.filter(p => p.id !== props.page))
const profiles = computed(() => snapshot.value?.profiles ?? [])
const servers = computed(() => (snapshot.value?.integrations ?? []).map(server => server.id))
const configurable = computed(() => hasTypeSettings(draft.value))

const name = computed(() => entry.value?.name ?? (draft.value.main?.type === 'plugin' ? t('actions.list.unknownPlugin') : t('editor.type.unknown')))

const detail = computed(() => {
  const key = draft.value
  switch (key.kind) {
    case 'action':
      return key.main ? summary(key.main) : ''
    case 'widget':
    case 'carousel':
      return widgetsOf(key).map(widget => widgetLabel(widget.kind)).join(' → ')
    case 'two_states':
      return t('editor.type.twoStatesSummary')
    case 'page_indicator':
      return t('editor.type.pageIndicatorSummary')
    default:
      return t('editor.type.basicSummary')
  }
})

const problem = computed(() => (draft.value.main ? actionsError([draft.value.main], servers.value) : null))

/** The action the key is. A key that follows it keeps the same in its second state. */
const main = computed({
  get: () => draft.value.main,
  set: (action: Action | null) => {
    const key = draft.value
    key.main = action
    if (key.second?.main && action) {
      key.second.main = JSON.parse(JSON.stringify(action)) as Action
    }
  },
})

const hasGestures = computed(() => Boolean(draft.value.doublePress.length || draft.value.longPress.length))

function unavailable(candidate: CatalogEntry) {
  if ((candidate.setup === 'two_states' || candidate.setup === 'switch_hotkey') && hasGestures.value) {
    return t('editor.inspector.twoStatesUnavailable')
  }
  if (candidate.navigates && draft.value.actions.some(action => entryFor(action)?.navigates)) {
    return t('actions.list.unavailable.navigatesOnce')
  }
  if (candidate.id === 'open_page' && !otherPages.value.length) {
    return t('actions.list.unavailable.noOtherPage')
  }
  if (candidate.id === 'switch_profile' && profiles.value.length < 2) {
    return t('actions.list.unavailable.oneProfile')
  }
  return null
}

/** Long enough for the catalog to finish closing: two dialogs at once fight over the focus. */
const AFTER_CATALOG_MS = 250

function change(candidate: CatalogEntry) {
  applyType(draft.value, candidate, {
    otherPages: otherPages.value.map(p => p.id),
    profiles: profiles.value.filter(p => p.id !== snapshot.value?.active_profile).map(p => p.id),
  })
  shownWidget.value = 0
  if (hasTypeSettings(draft.value)) {
    setTimeout(() => {
      configuring.value = true
    }, AFTER_CATALOG_MS)
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2 rounded-lg border bg-neutral-950/40 p-1.5 transition-colors hover:border-sky-500/50" data-key-type>
      <button
        type="button"
        class="flex min-w-0 flex-1 items-center gap-3 rounded-md p-1 text-left"
        :aria-label="t('editor.type.change')"
        :title="t('editor.type.change')"
        data-change-type
        @click="changing = true"
      >
        <img v-if="entry?.image" :src="entry.image" alt="" class="size-9 shrink-0 rounded-md bg-neutral-800 object-cover">
        <span v-else class="flex size-9 shrink-0 items-center justify-center rounded-md bg-sky-500/15 text-sky-400">
          <component :is="entry?.icon ?? CircleHelp" class="size-4.5" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="block truncate text-sm font-medium" data-key-type-name>{{ name }}</span>
          <span class="block truncate text-xs" :class="problem ? 'text-destructive' : 'text-muted-foreground'">{{ detail }}</span>
        </span>
        <ChevronsUpDown class="size-4 shrink-0 text-muted-foreground" />
      </button>
      <Button
        v-if="configurable"
        variant="outline"
        size="sm"
        :aria-label="t('editor.type.configureLabel', { name })"
        data-configure-type
        @click="configuring = true"
      >
        <Settings2 /> {{ t('editor.type.configure') }}
      </Button>
    </div>

    <ActionPicker v-model:open="changing" mode="type" :unavailable="unavailable" @pick="change" />

    <ActionDialog
      v-if="draft.kind === 'action' && main"
      v-model:open="configuring"
      v-model:action="main"
      :page="props.page"
      :other-pages="otherPages"
      face
    />

    <Dialog v-else-if="draft.kind === 'widget' || draft.kind === 'carousel'" v-model:open="configuring">
      <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl" data-type-dialog>
        <DialogHeader class="border-b px-5 pt-5 pb-4">
          <DialogTitle>{{ name }}</DialogTitle>
          <DialogDescription>{{ t(draft.kind === 'carousel' ? 'editor.type.carouselDescription' : 'editor.type.widgetDescription') }}</DialogDescription>
        </DialogHeader>
        <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <WidgetFields v-model="draft" v-model:shown="shownWidget" :carousel="draft.kind === 'carousel'" />
        </div>
        <div class="flex items-center justify-end gap-3 rounded-b-xl border-t bg-muted/50 px-5 py-3">
          <Button size="sm" @click="configuring = false">
            {{ t('actions.details.done') }}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  </div>
</template>
