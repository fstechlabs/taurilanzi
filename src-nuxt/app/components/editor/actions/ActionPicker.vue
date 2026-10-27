<script setup lang="ts">
import { Search, X } from '@lucide/vue'
import type { CatalogEntry, SectionId } from '~/lib/actions/catalog'
import { allEntries, allSections, matchesQuery } from '~/lib/actions/catalog'
import { os } from '~/lib/os'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{
  /** Inside a random action: only what can be one of its picks. */
  nested?: boolean
  /** Why an entry cannot be added right now, if it cannot. */
  unavailable?: (entry: CatalogEntry) => string | null
}>()
const emit = defineEmits<{ pick: [entry: CatalogEntry] }>()
const { t } = useI18n()

const query = ref('')
const section = ref<SectionId | 'all'>('all')
const showSoon = ref(true)
const onlyWithoutPermission = ref(false)

// A filter left on from last time would hide actions without saying why.
watch(open, (value) => {
  if (value) {
    clearFilters()
  }
})

const pool = computed(() => allEntries().filter(entry => !props.nested || entry.random))

const results = computed(() => pool.value.filter(entry =>
  (section.value === 'all' || entry.section === section.value)
  && (showSoon.value || !entry.soon)
  && (!onlyWithoutPermission.value || !entry.accessibility)
  && matchesQuery(entry, query.value),
))

const groups = computed(() => allSections()
  .map(s => ({ section: s, entries: results.value.filter(e => e.section === s.id) }))
  .filter(group => group.entries.length))

function count(id: SectionId | 'all') {
  return pool.value.filter(entry =>
    (id === 'all' || entry.section === id)
    && (showSoon.value || !entry.soon)
    && (!onlyWithoutPermission.value || !entry.accessibility)
    && matchesQuery(entry, query.value),
  ).length
}

const sections = computed(() => allSections().filter(s => pool.value.some(e => e.section === s.id)))

function reason(entry: CatalogEntry) {
  return entry.soon ?? props.unavailable?.(entry) ?? null
}

function pick(entry: CatalogEntry) {
  if (reason(entry)) {
    return
  }
  emit('pick', entry)
  open.value = false
}

/** Enter adds the first result that can be added. */
function pickFirst() {
  const first = results.value.find(entry => !reason(entry))
  if (first) {
    pick(first)
  }
}

const filtered = computed(() => Boolean(query.value) || section.value !== 'all' || !showSoon.value || onlyWithoutPermission.value)

function clearFilters() {
  query.value = ''
  section.value = 'all'
  showSoon.value = true
  onlyWithoutPermission.value = false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex h-[min(80vh,720px)] max-w-[min(92vw,960px)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(92vw,960px)]">
      <DialogHeader class="border-b px-5 pt-5 pb-4">
        <DialogTitle>{{ t(props.nested ? 'actions.list.addRandom' : 'actions.list.add') }}</DialogTitle>
        <DialogDescription>
          {{ t('actions.picker.description') }}
        </DialogDescription>
        <div class="relative mt-3">
          <Search class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            v-model="query"
            :placeholder="t('actions.picker.searchPlaceholder')"
            class="pl-8"
            autofocus
            :aria-label="t('actions.picker.search')"
            @keydown.enter.prevent="pickFirst"
          />
          <button
            v-if="query"
            type="button"
            class="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            :aria-label="t('actions.picker.clearSearch')"
            @click="query = ''"
          >
            <X class="size-4" />
          </button>
        </div>
      </DialogHeader>

      <div class="flex min-h-0 flex-1">
        <nav class="flex w-52 shrink-0 flex-col gap-0.5 overflow-y-auto border-r p-3" :aria-label="t('actions.picker.sections')">
          <button
            v-for="item in [{ id: 'all' as const, name: t('actions.picker.all') }, ...sections]"
            :key="item.id"
            type="button"
            class="flex items-center justify-between rounded-md px-2.5 py-1.5 text-left text-sm transition-colors"
            :class="section === item.id ? 'bg-neutral-800 text-foreground' : 'text-muted-foreground hover:bg-neutral-800/50 hover:text-foreground'"
            :aria-pressed="section === item.id"
            :data-section="item.id"
            @click="section = item.id"
          >
            <span class="truncate">{{ item.name }}</span>
            <span class="text-xs tabular-nums opacity-70">{{ count(item.id) }}</span>
          </button>

          <div class="mt-auto flex flex-col gap-3 border-t pt-3">
            <label class="flex items-center justify-between gap-2 text-xs text-muted-foreground">
              {{ t('actions.picker.showSoon') }}
              <Switch v-model="showSoon" :aria-label="t('actions.picker.showSoon')" />
            </label>
            <label v-if="os !== 'windows'" class="flex items-center justify-between gap-2 text-xs text-muted-foreground">
              {{ t('actions.picker.noPermissions') }}
              <Switch v-model="onlyWithoutPermission" :aria-label="t('actions.picker.noPermissions')" />
            </label>
            <Button v-if="filtered" variant="ghost" size="sm" class="justify-start" @click="clearFilters">
              <X /> {{ t('actions.picker.clearFilters') }}
            </Button>
          </div>
        </nav>

        <div class="min-h-0 flex-1 overflow-y-auto p-4" role="list" :aria-label="t('actions.picker.actions')">
          <p v-if="!groups.length" class="py-16 text-center text-sm text-muted-foreground">
            {{ t('actions.picker.noResults') }}
          </p>
          <section v-for="group in groups" :key="group.section.id" class="mb-5 last:mb-0">
            <header class="mb-2 flex items-baseline gap-2">
              <h3 class="text-sm font-semibold">
                {{ group.section.name }}
              </h3>
              <p class="text-xs text-muted-foreground">
                {{ group.section.description }}
              </p>
            </header>
            <div class="grid grid-cols-1 gap-2 md:grid-cols-2">
              <button
                v-for="entry in group.entries"
                :key="entry.id"
                type="button"
                role="listitem"
                class="flex items-start gap-3 rounded-lg border p-3 text-left transition-colors"
                :class="reason(entry) ? 'cursor-not-allowed opacity-50' : 'hover:border-sky-500/60 hover:bg-sky-500/5'"
                :aria-disabled="Boolean(reason(entry))"
                :aria-label="entry.name"
                :data-entry="entry.id"
                :title="reason(entry) ?? entry.description"
                @click="pick(entry)"
              >
                <img v-if="entry.image" :src="entry.image" alt="" class="size-8 shrink-0 rounded-md bg-neutral-800 object-cover">
                <div v-else class="flex size-8 shrink-0 items-center justify-center rounded-md bg-sky-500/15 text-sky-400">
                  <component :is="entry.icon" class="size-4" />
                </div>
                <div class="min-w-0 flex-1">
                  <p class="text-sm font-medium">
                    {{ entry.name }}
                  </p>
                  <p class="text-xs text-muted-foreground">
                    {{ reason(entry) && (!entry.soon || entry.plugin) ? reason(entry) : entry.description }}
                  </p>
                  <div v-if="entry.soon || entry.accessibility || entry.navigates || entry.plugin" class="mt-1.5 flex flex-wrap gap-1">
                    <Badge v-if="entry.soon" variant="secondary">
                      {{ t(entry.plugin ? 'actions.picker.unavailable' : 'actions.picker.soon') }}
                    </Badge>
                    <Badge v-if="entry.plugin" variant="outline">
                      {{ t('actions.picker.plugin') }}
                    </Badge>
                    <Badge v-if="entry.accessibility" variant="outline">
                      {{ t(`actions.picker.accessibility.${os}`) }}
                    </Badge>
                    <Badge v-if="entry.navigates" variant="outline">
                      {{ t('actions.picker.movesDock') }}
                    </Badge>
                  </div>
                </div>
              </button>
            </div>
          </section>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
