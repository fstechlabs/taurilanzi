<script setup lang="ts">
import { ArrowUpCircle, Check, ChevronDown, Download, ExternalLink, FileArchive, FolderOpen, FolderSearch, LoaderCircle, PackagePlus, Puzzle, RefreshCw, Search, ShieldAlert, Star, X } from '@lucide/vue'
import type { DeepReadonly } from 'vue'
import { openExternal } from '~/lib/external'
import { storeImage } from '~/lib/actions/plugins'
import { facetCounts, filtered, filterPlugins, kindsOf, SORTS, sourcesOf, STATUSES } from '~/lib/store-filters'
import type { StoreFilters, StoreSort } from '~/lib/store-filters'
import type { CatalogPlugin } from '~/types/engine'

type Plugin = DeepReadonly<CatalogPlugin>

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ installed: [id: string], added: [id: string] }>()

const { plugins, loading, failure, installing, sort: chosen, load, install } = useStorePlugins()
const { install: installFrom, reload: reloadFolder, showFolder } = usePlugins()
const { show } = useToasts()
const { t, locale } = useI18n()

const filters = reactive<StoreFilters>({ query: '', source: 'all', status: 'all', kind: 'all' })
/** The order while searching: by relevance, unless another is picked then. */
const searchSort = ref<StoreSort>('relevance')
const searching = computed(() => filters.query.trim() !== '')
const sort = computed<StoreSort>({
  get: () => (searching.value ? searchSort.value : chosen.value),
  set: (value) => {
    if (searching.value) {
      searchSort.value = value
    }
    else {
      chosen.value = value
    }
  },
})
const sorts = computed<StoreSort[]>(() => (searching.value ? ['relevance', ...SORTS] : SORTS))
watch(searching, (now) => {
  if (!now) {
    searchSort.value = 'relevance'
  }
})
const picked = ref<string | null>(null)
const search = ref<{ $el: HTMLInputElement } | null>(null)

watch(open, (value) => {
  if (value) {
    Object.assign(filters, { query: '', source: 'all', status: 'all', kind: 'all' })
    picked.value = null
    load()
  }
}, { immediate: true })

/** The store and the category in the user's language, so a search finds them too. */
function labels(plugin: { source: string, kind: string | null }) {
  return [t(`plugins.store.from.${plugin.source}`), t(`plugins.store.kinds.${plugin.kind ?? 'other'}`)]
}

const all = computed(() => plugins.value ?? [])
const shown = computed(() => filterPlugins(all.value, filters, sort.value, labels))
const counts = computed(() => facetCounts(all.value, filters, labels))
const sources = computed(() => sourcesOf(all.value))
const kinds = computed(() => kindsOf(all.value))
const anyFilter = computed(() => filtered(filters))

const selected = computed(() => shown.value.find(p => p.id === picked.value) ?? shown.value[0] ?? null)

function count(value: number | undefined) {
  return new Intl.NumberFormat(locale.value).format(value ?? 0)
}

function clearFilters() {
  Object.assign(filters, { source: 'all', status: 'all', kind: 'all' })
}

function clearSearch() {
  filters.query = ''
  search.value?.$el.focus()
}

function downloads(n: number) {
  return t('plugins.store.downloads', { downloads: count(n) }, n)
}

function warned(plugin: Plugin) {
  return plugin.security.status === 'findings'
}

function findings(plugin: Plugin) {
  const { critical, high, secrets } = plugin.security
  return [
    critical && t('plugins.store.critical', { n: critical }),
    high && t('plugins.store.high', { n: high }),
    secrets && t('plugins.store.secrets', { n: secrets }),
  ].filter(Boolean).join(', ') || t('plugins.store.noDetail')
}

/** A plugin from a file or a folder: the store closes and the plugin shows. */
async function addFrom(kind: 'zip' | 'folder') {
  try {
    const id = await installFrom(kind)
    if (id) {
      open.value = false
      emit('added', id)
    }
  }
  catch (err) {
    show(t('plugins.installFailed'), err instanceof Error ? err.message : String(err))
  }
}

/** Plugins copied into the folder by hand, and which of the store's they are. */
async function rescan() {
  await reloadFolder()
  await load()
}

/** What is waiting for the user to accept running code nobody checked. */
type Pending = { kind: 'store', plugin: Plugin } | { kind: 'zip' | 'folder' }
const pending = ref<Pending | null>(null)
const confirming = computed({
  get: () => pending.value !== null,
  set: (value) => {
    if (!value) {
      pending.value = null
    }
  },
})

/** Only a plugin the community store checked and found clean installs without asking; an update
 * is of one already trusted. */
function needsConsent(plugin: Plugin) {
  return !plugin.installed && !(plugin.source === 'community' && plugin.security.status === 'clean')
}

function consentText(waiting: Pending) {
  if (waiting.kind !== 'store') {
    return t('plugins.store.consent.manual')
  }
  const { plugin } = waiting
  const store = t(`plugins.store.from.${plugin.source}`)
  const who = plugin.author || store
  if (warned(plugin)) {
    return t('plugins.store.consent.findings', { who })
  }
  return t('plugins.store.consent.unchecked', { store, who })
}

function ask(plugin: Plugin) {
  if (needsConsent(plugin)) {
    pending.value = { kind: 'store', plugin }
  }
  else {
    add(plugin)
  }
}

function accept() {
  const waiting = pending.value
  pending.value = null
  if (waiting?.kind === 'store') {
    add(waiting.plugin)
  }
  else if (waiting) {
    addFrom(waiting.kind)
  }
}

async function add(plugin: Plugin) {
  try {
    const id = await install(plugin.id)
    show(plugin.update ? t('plugins.updated') : t('plugins.installed'), t('plugins.store.ready', { name: plugin.name }))
    emit('installed', id)
  }
  catch (err) {
    show(t('plugins.installFailed'), err instanceof Error ? err.message : String(err))
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent
      class="flex h-[85vh] max-h-[780px] flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
      data-store-dialog
      @open-auto-focus.prevent="search?.$el.focus()"
    >
      <DialogHeader class="flex-row items-start gap-4 border-b px-5 pt-5 pb-4">
        <div class="min-w-0 flex-1">
          <DialogTitle>{{ t('plugins.store.title') }}</DialogTitle>
          <DialogDescription>
            {{ t('plugins.store.description') }}
          </DialogDescription>
        </div>
      </DialogHeader>
      <div class="flex min-h-0 flex-1">
        <div class="flex w-64 shrink-0 flex-col border-r">
          <nav class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3" :aria-label="t('plugins.store.filters')" data-store-filters>
            <div class="flex h-7 shrink-0 items-center justify-between px-2">
              <p class="text-xs font-medium">
                {{ t('plugins.store.filters') }}
              </p>
              <!-- Always there, hidden when unset, so nothing below moves as it shows. -->
              <Button
                variant="ghost"
                size="sm"
                class="-mr-2 h-7 px-2 text-xs text-muted-foreground"
                :class="{ invisible: !anyFilter }"
                :aria-hidden="!anyFilter"
                :tabindex="anyFilter ? 0 : -1"
                data-store-clear
                @click="clearFilters"
              >
                {{ t('plugins.store.clear') }}
              </Button>
            </div>
            <section class="flex shrink-0 flex-col gap-0.5" :aria-label="t('plugins.store.statusLabel')">
              <p class="px-2 pb-1 text-[11px] font-medium text-muted-foreground uppercase">
                {{ t('plugins.store.statusLabel') }}
              </p>
              <button
                v-for="option in STATUSES"
                :key="option"
                type="button"
                class="flex items-center gap-2 rounded-md px-2 py-1 text-left text-sm hover:bg-neutral-800/60 disabled:pointer-events-none disabled:opacity-40"
                :class="{ 'bg-neutral-800 font-medium': filters.status === option }"
                :aria-pressed="filters.status === option"
                :disabled="option !== 'all' && !counts.statuses.get(option) && filters.status !== option"
                :data-store-status="option"
                @click="filters.status = option"
              >
                <span class="min-w-0 flex-1 truncate">{{ t(`plugins.store.statuses.${option}`) }}</span>
                <span class="text-xs text-muted-foreground tabular-nums">{{ count(counts.statuses.get(option)) }}</span>
              </button>
            </section>
            <section class="flex shrink-0 flex-col gap-0.5 border-t pt-4" :aria-label="t('plugins.store.sourceLabel')">
              <p class="px-2 pb-1 text-[11px] font-medium text-muted-foreground uppercase">
                {{ t('plugins.store.sourceLabel') }}
              </p>
              <button
                v-for="option in ['all', ...sources] as const"
                :key="option"
                type="button"
                class="flex items-center gap-2 rounded-md px-2 py-1 text-left text-sm hover:bg-neutral-800/60 disabled:pointer-events-none disabled:opacity-40"
                :class="{ 'bg-neutral-800 font-medium': filters.source === option }"
                :aria-pressed="filters.source === option"
                :disabled="option !== 'all' && !counts.sources.get(option) && filters.source !== option"
                :data-store-source="option"
                @click="filters.source = option"
              >
                <span class="min-w-0 flex-1 truncate">{{ t(option === 'all' ? 'plugins.store.allSources' : `plugins.store.from.${option}`) }}</span>
                <span class="text-xs text-muted-foreground tabular-nums">{{ count(counts.sources.get(option)) }}</span>
              </button>
            </section>
            <section class="flex shrink-0 flex-col gap-0.5 border-t pt-4" :aria-label="t('plugins.store.kindLabel')">
              <p class="px-2 pb-1 text-[11px] font-medium text-muted-foreground uppercase">
                {{ t('plugins.store.kindLabel') }}
              </p>
              <button
                v-for="option in ['all', ...kinds] as const"
                :key="option"
                type="button"
                class="flex items-center gap-2 rounded-md px-2 py-1 text-left text-sm hover:bg-neutral-800/60 disabled:pointer-events-none disabled:opacity-40"
                :class="{ 'bg-neutral-800 font-medium': filters.kind === option }"
                :aria-pressed="filters.kind === option"
                :disabled="option !== 'all' && !counts.kinds.get(option) && filters.kind !== option"
                :data-store-kind="option"
                @click="filters.kind = option"
              >
                <span class="min-w-0 flex-1 truncate">{{ t(`plugins.store.kinds.${option}`) }}</span>
                <span class="text-xs text-muted-foreground tabular-nums">{{ count(counts.kinds.get(option)) }}</span>
              </button>
            </section>
          </nav>
          <div class="flex shrink-0 flex-col gap-2 border-t p-3">
            <Button variant="outline" size="sm" class="w-full" :disabled="loading" data-store-reload @click="load(true)">
              <RefreshCw :class="{ 'animate-spin': loading }" />
              {{ t('plugins.store.reload') }}
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <Button variant="outline" size="sm" class="w-full">
                  <PackagePlus /> {{ t('plugins.store.manual') }} <ChevronDown class="ml-auto" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" class="w-60">
                <DropdownMenuItem @select="pending = { kind: 'zip' }">
                  <FileArchive /> {{ t('plugins.list.fromZip') }}
                </DropdownMenuItem>
                <DropdownMenuItem @select="pending = { kind: 'folder' }">
                  <FolderSearch /> {{ t('plugins.list.fromFolder') }}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem @select="rescan()">
                  <RefreshCw /> {{ t('plugins.list.reload') }}
                </DropdownMenuItem>
                <DropdownMenuItem @select="showFolder()">
                  <FolderOpen /> {{ t('plugins.list.openFolder') }}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        <div class="flex w-80 shrink-0 flex-col border-r">
          <div class="flex flex-col gap-2 border-b p-3">
            <div class="relative">
              <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                ref="search"
                v-model="filters.query"
                :placeholder="t('plugins.store.search')"
                class="h-8 pr-8 pl-8 text-sm"
                :aria-label="t('plugins.store.searchLabel')"
                @keydown.esc.stop="filters.query ? clearSearch() : (open = false)"
              />
              <button
                v-if="filters.query"
                type="button"
                class="absolute top-1/2 right-1.5 flex size-5 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:text-foreground"
                :aria-label="t('plugins.store.clearSearch')"
                @click="clearSearch"
              >
                <X class="size-3.5" />
              </button>
            </div>
            <div class="flex items-center gap-2">
              <p class="flex-1 text-xs text-muted-foreground" data-store-count>
                {{ t('plugins.store.count', { n: count(shown.length) }, shown.length) }}
              </p>
              <Select v-model="sort">
                <SelectTrigger size="sm" class="h-7 text-xs" :aria-label="t('plugins.store.sortLabel')" data-store-sort>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="option in sorts" :key="option" :value="option" class="text-xs">
                    {{ t(`plugins.store.sorts.${option}`) }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto p-2">
            <p v-if="failure && !plugins" class="px-2 py-3 text-xs text-muted-foreground">
              {{ t('plugins.store.loadFailed') }}
            </p>
            <p v-else-if="!plugins" class="flex items-center gap-2 px-2 py-3 text-xs text-muted-foreground">
              <LoaderCircle class="size-3.5 animate-spin" /> {{ t('plugins.store.loading') }}
            </p>
            <div v-else-if="!shown.length" class="flex flex-col items-start gap-2 px-2 py-3 text-xs text-muted-foreground">
              {{ t('plugins.store.noMatch') }}
              <Button v-if="anyFilter" variant="outline" size="sm" class="h-7 text-xs" @click="clearFilters">
                {{ t('plugins.store.clear') }}
              </Button>
            </div>
            <button
              v-for="plugin in shown"
              :key="plugin.id"
              type="button"
              class="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-neutral-800/60"
              :class="{ 'bg-neutral-800': selected?.id === plugin.id }"
              :data-store-plugin="plugin.id"
              @click="picked = plugin.id"
            >
              <img v-if="plugin.icon" :src="storeImage(plugin.icon)" alt="" class="size-9 shrink-0 rounded-md bg-neutral-900 object-contain">
              <Puzzle v-else class="size-9 shrink-0 p-1 text-muted-foreground" />
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm">
                  {{ plugin.name }}
                </p>
                <p class="truncate text-[11px] text-muted-foreground">
                  {{ [plugin.author, t(`plugins.store.from.${plugin.source}`)].filter(Boolean).join(' · ') }}
                </p>
              </div>
              <ArrowUpCircle v-if="plugin.update" class="size-4 shrink-0 text-sky-400" :aria-label="t('plugins.updateAvailable')" />
              <Check v-else-if="plugin.installed" class="size-4 shrink-0 text-emerald-500" :aria-label="t('plugins.store.installed')" />
            </button>
          </div>
        </div>

        <div v-if="selected" class="min-w-0 flex-1 overflow-y-auto px-6 py-5" data-store-details>
          <div class="flex items-start gap-4">
            <img v-if="selected.icon" :src="storeImage(selected.icon)" alt="" class="size-16 shrink-0 rounded-xl bg-neutral-900 object-contain">
            <Puzzle v-else class="size-16 shrink-0 p-2 text-muted-foreground" />
            <div class="min-w-0 flex-1">
              <h3 class="truncate text-lg font-semibold">
                {{ selected.name }}
              </h3>
              <p class="text-sm text-muted-foreground">
                {{ [selected.author, t('plugins.version', { version: selected.version }), downloads(selected.downloads)].join(' · ') }}
                <span v-if="selected.stars" class="inline-flex items-center gap-0.5"> · <Star class="size-3" /> {{ selected.stars }}</span>
              </p>
              <Badge variant="outline" class="mt-2" data-store-origin>
                {{ t(`plugins.store.from.${selected.source}`) }}
              </Badge>
              <div class="mt-3 flex flex-wrap items-center gap-2">
                <Button
                  v-if="!selected.installed || selected.update"
                  size="sm"
                  :disabled="installing.has(selected.id)"
                  @click="ask(selected)"
                >
                  <LoaderCircle v-if="installing.has(selected.id)" class="animate-spin" />
                  <ArrowUpCircle v-else-if="selected.update" />
                  <Download v-else />
                  {{ installing.has(selected.id) ? t('plugins.store.installing') : selected.update ? t('plugins.updateTo', { version: selected.version }) : t('plugins.store.install') }}
                </Button>
                <Badge v-else variant="outline" class="h-8 gap-1 px-3" data-store-installed>
                  <Check class="size-3.5 text-emerald-500" /> {{ t('plugins.store.installed') }}
                </Badge>
                <Button v-if="selected.source_url" variant="ghost" size="sm" @click="openExternal(selected.source_url)">
                  <ExternalLink /> {{ t('plugins.store.source') }}
                </Button>
              </div>
            </div>
          </div>

          <div v-if="warned(selected)" class="mt-4 flex gap-2.5 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-200">
            <ShieldAlert class="size-4 shrink-0" />
            <p>
              {{ t('plugins.store.warning', { findings: findings(selected) }) }}
            </p>
          </div>

          <p class="mt-4 text-sm">
            {{ selected.description }}
          </p>
          <p v-if="selected.long_description && selected.long_description !== selected.description" class="mt-2 text-sm whitespace-pre-line text-muted-foreground">
            {{ selected.long_description }}
          </p>
          <p v-if="selected.dials" class="mt-2 text-xs text-muted-foreground">
            {{ t('plugins.store.dials') }}
          </p>

          <div v-if="selected.screenshots.length" class="mt-4 flex gap-2 overflow-x-auto pb-1">
            <img
              v-for="shot in selected.screenshots"
              :key="shot"
              :src="storeImage(shot)"
              alt=""
              class="h-44 shrink-0 rounded-md border bg-neutral-900 object-contain"
            >
          </div>

          <div v-if="selected.tags.length || selected.kind || selected.category" class="mt-4 flex flex-wrap gap-1.5">
            <Badge v-if="selected.kind || selected.category" variant="secondary">
              {{ selected.kind ? t(`plugins.store.kinds.${selected.kind}`) : selected.category }}
            </Badge>
            <Badge v-for="tag in selected.tags" :key="tag" variant="outline">
              {{ tag }}
            </Badge>
          </div>
        </div>
        <div v-else class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          <Puzzle class="mr-2 size-4" /> {{ t('plugins.store.choose') }}
        </div>
      </div>
    </DialogContent>
  </Dialog>

  <Dialog v-model:open="confirming">
    <DialogContent class="sm:max-w-md" data-store-consent>
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2">
          <ShieldAlert class="size-4 text-amber-400" />
          {{ pending?.kind === 'store' ? t('plugins.store.consent.title', { name: pending.plugin.name }) : t('plugins.store.consent.manualTitle') }}
        </DialogTitle>
        <DialogDescription class="leading-relaxed">
          {{ t('plugins.store.consent.risk') }}
        </DialogDescription>
      </DialogHeader>
      <p v-if="pending" class="text-sm">
        {{ consentText(pending) }}
      </p>
      <DialogFooter>
        <Button variant="ghost" @click="pending = null">
          {{ t('plugins.store.consent.cancel') }}
        </Button>
        <Button @click="accept">
          {{ t('plugins.store.consent.accept') }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
