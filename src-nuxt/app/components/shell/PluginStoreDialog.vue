<script setup lang="ts">
import { ArrowUpCircle, Check, Download, ExternalLink, LoaderCircle, Puzzle, RefreshCw, Search, ShieldAlert, Star } from '@lucide/vue'
import type { DeepReadonly } from 'vue'
import { openExternal } from '~/lib/external'
import { storeImage } from '~/lib/actions/plugins'
import { facetCounts, filterPlugins, KINDS, SORTS, sourcesOf, STATUSES } from '~/lib/store-filters'
import type { StoreFilters, StoreSort } from '~/lib/store-filters'
import type { CatalogPlugin } from '~/types/engine'

type Plugin = DeepReadonly<CatalogPlugin>

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ installed: [id: string] }>()

const { plugins, loading, failure, installing, load, install } = useStorePlugins()
const { show } = useToasts()
const { t, locale } = useI18n()

const filters = reactive<StoreFilters>({ query: '', source: 'all', status: 'all', kind: 'all' })
const sort = ref<StoreSort>('popular')
const chosen = ref<string | null>(null)

watch(open, (value) => {
  if (value) {
    load()
  }
}, { immediate: true })

const all = computed(() => plugins.value ?? [])
const shown = computed(() => filterPlugins(all.value, filters, sort.value))
const counts = computed(() => facetCounts(all.value, filters))
const sources = computed(() => sourcesOf(all.value))
const filtered = computed(() => filters.query.trim() !== '' || filters.source !== 'all' || filters.status !== 'all' || filters.kind !== 'all')

const selected = computed(() => shown.value.find(p => p.id === chosen.value) ?? shown.value[0] ?? null)

function count(value: number | undefined) {
  return new Intl.NumberFormat(locale.value).format(value ?? 0)
}

function clear() {
  Object.assign(filters, { query: '', source: 'all', status: 'all', kind: 'all' })
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
    <DialogContent class="flex h-[85vh] max-h-[780px] flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl" data-store-dialog>
      <DialogHeader class="flex-row items-start gap-4 border-b px-5 pt-5 pb-4">
        <div class="min-w-0 flex-1">
          <DialogTitle>{{ t('plugins.store.title') }}</DialogTitle>
          <DialogDescription>
            {{ t('plugins.store.description') }}
          </DialogDescription>
        </div>
        <div class="mr-8 flex w-80 shrink-0 items-center gap-1.5">
          <div class="relative flex-1">
            <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input v-model="filters.query" :placeholder="t('plugins.store.search')" class="h-8 pl-8 text-sm" :aria-label="t('plugins.store.searchLabel')" />
          </div>
          <Button variant="ghost" size="icon" class="size-8" :aria-label="t('plugins.store.reload')" :disabled="loading" @click="load(true)">
            <RefreshCw :class="{ 'animate-spin': loading }" />
          </Button>
        </div>
      </DialogHeader>
      <div class="flex min-h-0 flex-1">
        <nav class="flex w-64 shrink-0 flex-col gap-4 overflow-y-auto border-r p-3" :aria-label="t('plugins.store.filters')" data-store-filters>
          <section class="flex flex-col gap-0.5" :aria-label="t('plugins.store.sourceLabel')">
            <p class="px-2 pb-1 text-[11px] font-medium text-muted-foreground uppercase">
              {{ t('plugins.store.sourceLabel') }}
            </p>
            <button
              v-for="option in ['all', ...sources] as const"
              :key="option"
              type="button"
              class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-neutral-800/60"
              :class="{ 'bg-neutral-800 font-medium': filters.source === option }"
              :aria-pressed="filters.source === option"
              :data-store-source="option"
              @click="filters.source = option"
            >
              <span class="min-w-0 flex-1 truncate">{{ t(option === 'all' ? 'plugins.store.allSources' : `plugins.store.from.${option}`) }}</span>
              <span class="text-xs text-muted-foreground tabular-nums">{{ count(counts.sources.get(option)) }}</span>
            </button>
          </section>
          <section class="flex flex-col gap-0.5" :aria-label="t('plugins.store.statusLabel')">
            <p class="px-2 pb-1 text-[11px] font-medium text-muted-foreground uppercase">
              {{ t('plugins.store.statusLabel') }}
            </p>
            <button
              v-for="option in STATUSES"
              :key="option"
              type="button"
              class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-neutral-800/60"
              :class="{ 'bg-neutral-800 font-medium': filters.status === option }"
              :aria-pressed="filters.status === option"
              :data-store-status="option"
              @click="filters.status = option"
            >
              <span class="min-w-0 flex-1 truncate">{{ t(`plugins.store.statuses.${option}`) }}</span>
              <span class="text-xs text-muted-foreground tabular-nums">{{ count(counts.statuses.get(option)) }}</span>
            </button>
          </section>
          <section class="flex flex-col gap-0.5" :aria-label="t('plugins.store.kindLabel')">
            <p class="px-2 pb-1 text-[11px] font-medium text-muted-foreground uppercase">
              {{ t('plugins.store.kindLabel') }}
            </p>
            <button
              v-for="option in ['all', ...KINDS] as const"
              :key="option"
              type="button"
              class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-neutral-800/60 disabled:pointer-events-none disabled:opacity-40"
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

        <div class="flex w-80 shrink-0 flex-col border-r">
          <div class="flex h-11 items-center gap-2 border-b px-3">
            <p class="flex-1 text-xs text-muted-foreground" data-store-count>
              {{ t('plugins.store.count', { n: count(shown.length) }, shown.length) }}
            </p>
            <Button v-if="filtered" variant="ghost" size="sm" class="h-7 px-2 text-xs" @click="clear">
              {{ t('plugins.store.clear') }}
            </Button>
            <Select v-model="sort">
              <SelectTrigger size="sm" class="text-xs" :aria-label="t('plugins.store.sortLabel')">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem v-for="option in SORTS" :key="option" :value="option" class="text-xs">
                  {{ t(`plugins.store.sorts.${option}`) }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto p-2">
            <p v-if="failure && !plugins" class="px-2 py-3 text-xs text-muted-foreground">
              {{ t('plugins.store.loadFailed') }}
            </p>
            <p v-else-if="!plugins" class="flex items-center gap-2 px-2 py-3 text-xs text-muted-foreground">
              <LoaderCircle class="size-3.5 animate-spin" /> {{ t('plugins.store.loading') }}
            </p>
            <p v-else-if="!shown.length" class="px-2 py-3 text-xs text-muted-foreground">
              {{ t('plugins.store.noMatch') }}
            </p>
            <button
              v-for="plugin in shown"
              :key="plugin.id"
              type="button"
              class="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-neutral-800/60"
              :class="{ 'bg-neutral-800': selected?.id === plugin.id }"
              :data-store-plugin="plugin.id"
              @click="chosen = plugin.id"
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
                  @click="add(selected)"
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
</template>
