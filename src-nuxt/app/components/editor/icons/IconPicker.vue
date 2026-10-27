<script setup lang="ts">
import { Loader2, Search, X } from '@lucide/vue'
import type { IconBrowsePage, IconPalette, IconSearchResult, IconStyle } from '~/lib/iconify'
import { browseCollection, preload, searchIcons } from '~/lib/iconify'
import type { PickedIcon } from '~/composables/useIconCatalog'
import { ANY, categoryLabel, iconesCollectionUrl } from '~/composables/useIconCatalog'

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ pick: [picked: PickedIcon] }>()
const catalog = useIconCatalog()
const { t, locale } = useI18n()

const SUGGESTIONS = [
  'home', 'play', 'pause', 'microphone', 'volume', 'camera', 'lightbulb', 'lock',
  'music', 'settings', 'terminal', 'github', 'spotify', 'discord', 'twitch', 'youtube',
]

const query = ref('')
const prefix = ref(ANY)
const category = ref(ANY)
const style = ref<IconStyle | typeof ANY>(ANY)
const palette = ref<IconPalette | typeof ANY>(ANY)
const browseCategory = ref(ANY)

const found = ref<IconSearchResult>({ icons: [], collections: {}, truncated: false })
const browsed = ref<IconBrowsePage | null>(null)
const status = ref<'idle' | 'loading' | 'more' | 'error'>('idle')
const active = ref<string | null>(null)

const mode = computed(() => query.value.trim() ? 'search' : prefix.value !== ANY ? 'browse' : 'home')
const collection = computed(() => catalog.byPrefix.value.get(prefix.value))
const filtered = computed(() =>
  category.value !== ANY || style.value !== ANY || palette.value !== ANY)
const resultCount = computed(() => {
  const n = found.value.icons.length
  return t('icons.picker.count', { total: `${n}${found.value.truncated ? '+' : ''}` }, n)
})
const icons = computed(() => {
  switch (mode.value) {
    case 'search': return found.value.icons
    case 'browse': return browsed.value?.icons ?? []
    default: return catalog.recent.value
  }
})

/** Collections present in the results, biggest first, to narrow a search with one click. */
const resultCollections = computed(() => {
  const counts = new Map<string, number>()
  for (const icon of found.value.icons) {
    const p = icon.split(':')[0] ?? ''
    counts.set(p, (counts.get(p) ?? 0) + 1)
  }
  return [...counts]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([p, count]) => ({ prefix: p, count, name: found.value.collections[p] ?? catalog.byPrefix.value.get(p)?.name ?? p }))
})

watch(open, (visible) => {
  if (visible) {
    query.value = ''
    catalog.load()
    preload(catalog.recent.value)
  }
})

let timer: ReturnType<typeof setTimeout> | undefined
let generation = 0

watch([query, prefix, category, style, palette, browseCategory], () => {
  clearTimeout(timer)
  timer = setTimeout(refresh, 250)
})
watch(prefix, () => { browseCategory.value = ANY })

async function refresh() {
  const run = ++generation
  active.value = null
  if (mode.value === 'home') {
    status.value = 'idle'
    return
  }
  status.value = 'loading'
  try {
    if (mode.value === 'search') {
      const result = await searchIcons(query.value, prefix.value === ANY ? null : prefix.value, filters())
      if (run === generation) {
        found.value = result
      }
    }
    else {
      const page = await browse(0)
      if (run === generation) {
        browsed.value = page
      }
    }
    if (run === generation) {
      status.value = 'idle'
    }
  }
  catch {
    if (run === generation) {
      status.value = 'error'
    }
  }
}

function filters() {
  return {
    category: category.value === ANY || prefix.value !== ANY ? undefined : category.value,
    style: style.value === ANY ? undefined : style.value,
    palette: palette.value === ANY ? undefined : palette.value,
  }
}

function browse(page: number) {
  return browseCollection(prefix.value, browseCategory.value === ANY ? null : browseCategory.value, page)
}

async function loadMore() {
  const current = browsed.value
  if (!current || current.page + 1 >= current.pages) {
    return
  }
  const run = generation
  status.value = 'more'
  try {
    const next = await browse(current.page + 1)
    if (run === generation) {
      browsed.value = { ...next, icons: [...current.icons, ...next.icons] }
    }
  }
  finally {
    if (run === generation) {
      status.value = 'idle'
    }
  }
}

function setStyle(value: unknown) {
  style.value = (value as IconStyle | undefined) ?? ANY
}

function setPalette(value: unknown) {
  palette.value = (value as IconPalette | undefined) ?? ANY
}

function clearFilters() {
  category.value = ANY
  style.value = ANY
  palette.value = ANY
}

function pick(icon: string) {
  catalog.remember(icon)
  emit('pick', { icon, multicolor: catalog.isMulticolor(icon) })
  open.value = false
}

function count(value: number) {
  return value.toLocaleString(locale.value)
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="gap-3 sm:max-w-4xl">
      <DialogHeader>
        <DialogTitle>{{ t('icons.picker.title') }}</DialogTitle>
        <DialogDescription>
          <i18n-t keypath="icons.picker.description" scope="global">
            <template #home>
              <code>home</code>
            </template>
            <template #play>
              <code>play</code>
            </template>
            <template #key>
              <code>mdi:home</code>
            </template>
          </i18n-t>
        </DialogDescription>
      </DialogHeader>

      <div class="flex gap-2">
        <div class="relative flex-1">
          <Search class="absolute top-2.5 left-2.5 size-4 text-muted-foreground" />
          <Input v-model="query" placeholder="home, github, mdi:home…" class="pr-8 pl-8" autofocus :aria-label="t('icons.picker.search')" />
          <button
            v-if="query"
            type="button"
            class="absolute top-2 right-2 rounded-sm text-muted-foreground hover:text-foreground"
            :aria-label="t('icons.picker.clear')"
            @click="query = ''"
          >
            <X class="size-4" />
          </button>
        </div>
        <IconCollectionSelect v-model="prefix" :collections="catalog.collections.value" :categories="catalog.categories.value" />
      </div>

      <div class="flex flex-wrap items-center gap-2 text-xs">
        <template v-if="mode !== 'browse'">
          <Select v-model="category" :disabled="prefix !== ANY">
            <SelectTrigger size="sm" class="w-48" :aria-label="t('icons.picker.kind')">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem :value="ANY">
                {{ t('icons.picker.anyKind') }}
              </SelectItem>
              <SelectItem v-for="c in catalog.categories.value.filter(Boolean)" :key="c" :value="c">
                {{ categoryLabel(c) }}
              </SelectItem>
            </SelectContent>
          </Select>
          <ToggleGroup :model-value="style" type="single" variant="outline" size="sm" :aria-label="t('icons.picker.style')" @update:model-value="setStyle">
            <ToggleGroupItem :value="ANY">
              {{ t('icons.picker.anyStyle') }}
            </ToggleGroupItem>
            <ToggleGroupItem value="fill">
              {{ t('icons.picker.fill') }}
            </ToggleGroupItem>
            <ToggleGroupItem value="stroke">
              {{ t('icons.picker.stroke') }}
            </ToggleGroupItem>
          </ToggleGroup>
          <ToggleGroup :model-value="palette" type="single" variant="outline" size="sm" :aria-label="t('icons.picker.color')" @update:model-value="setPalette">
            <ToggleGroupItem :value="ANY">
              {{ t('icons.picker.anyColor') }}
            </ToggleGroupItem>
            <ToggleGroupItem value="mono" :title="t('icons.picker.monoHint')">
              {{ t('icons.picker.mono') }}
            </ToggleGroupItem>
            <ToggleGroupItem value="color" :title="t('icons.picker.multicolorHint')">
              {{ t('icons.picker.multicolor') }}
            </ToggleGroupItem>
          </ToggleGroup>
          <Button v-if="filtered" variant="ghost" size="sm" @click="clearFilters">
            <X /> {{ t('icons.picker.clearFilters') }}
          </Button>
        </template>
        <template v-else-if="collection">
          <Select v-if="browsed && Object.keys(browsed.categories).length" v-model="browseCategory">
            <SelectTrigger size="sm" class="w-56" :aria-label="t('icons.picker.category')">
              <SelectValue />
            </SelectTrigger>
            <SelectContent class="max-h-80">
              <SelectItem :value="ANY">
                {{ t('icons.picker.allIcons') }}
              </SelectItem>
              <SelectItem v-for="(n, c) in browsed.categories" :key="c" :value="c">
                {{ c }} ({{ n }})
              </SelectItem>
            </SelectContent>
          </Select>
          <span class="text-muted-foreground">
            {{ t('icons.picker.count', { total: count(collection.total) }, collection.total) }}<template v-if="collection.author"> · {{ collection.author }}</template>
            <template v-if="collection.license"> · {{ collection.license.spdx ?? collection.license.title }}</template>
          </span>
          <ExternalLink :href="iconesCollectionUrl(collection.prefix)" class="ml-auto">
            {{ t('icons.picker.openIcones') }}
          </ExternalLink>
        </template>
      </div>

      <div v-if="mode === 'search' && resultCollections.length > 1 && prefix === ANY" class="flex flex-wrap gap-1.5">
        <button
          v-for="c in resultCollections"
          :key="c.prefix"
          type="button"
          class="rounded-full border px-2.5 py-0.5 text-xs text-muted-foreground transition hover:border-primary hover:text-foreground"
          @click="prefix = c.prefix"
        >
          {{ c.name }} <span class="tabular-nums opacity-60">{{ c.count }}</span>
        </button>
      </div>

      <div class="h-80 overflow-y-auto rounded-md border p-2">
        <div v-if="status === 'loading'" class="flex h-full items-center justify-center text-sm text-muted-foreground">
          <Loader2 class="mr-2 size-4 animate-spin" /> {{ t('icons.picker.searching') }}
        </div>
        <div v-else-if="status === 'error'" class="flex h-full items-center justify-center text-sm text-muted-foreground">
          {{ t('icons.picker.error') }}
        </div>
        <div v-else-if="mode === 'home'" class="flex flex-col gap-4 p-1">
          <section v-if="icons.length" class="flex flex-col gap-2">
            <h3 class="text-xs font-medium text-muted-foreground">
              {{ t('icons.picker.recent') }}
            </h3>
            <IconGrid :icons="icons" @pick="pick" @focus="active = $event" />
          </section>
          <section class="flex flex-col gap-2">
            <h3 class="text-xs font-medium text-muted-foreground">
              {{ t('icons.picker.ideas') }}
            </h3>
            <div class="flex flex-wrap gap-1.5">
              <button
                v-for="s in SUGGESTIONS"
                :key="s"
                type="button"
                class="rounded-full border px-2.5 py-0.5 text-xs transition hover:border-primary"
                @click="query = s"
              >
                {{ s }}
              </button>
            </div>
          </section>
        </div>
        <div v-else-if="!icons.length" class="flex h-full flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
          {{ t('icons.picker.noResults') }}
          <Button v-if="filtered" variant="outline" size="sm" @click="clearFilters">
            {{ t('icons.picker.clearFilters') }}
          </Button>
        </div>
        <template v-else>
          <IconGrid :icons="icons" @pick="pick" @focus="active = $event" />
          <p v-if="mode === 'search' && found.truncated" class="pt-3 text-center text-xs text-muted-foreground">
            {{ t('icons.picker.truncated', { shown: icons.length }) }}
          </p>
          <div v-if="mode === 'browse' && browsed && browsed.page + 1 < browsed.pages" class="flex justify-center pt-3">
            <Button variant="outline" size="sm" :disabled="status === 'more'" @click="loadMore">
              <Loader2 v-if="status === 'more'" class="animate-spin" />
              {{ t('icons.picker.loadMore', { left: count(browsed.total - icons.length) }) }}
            </Button>
          </div>
        </template>
      </div>

      <div class="flex min-h-10 items-center justify-between gap-4">
        <IconDetails v-if="active" :icon="active" :collection="catalog.byPrefix.value.get(active.split(':')[0] ?? '')" />
        <span v-else class="text-xs text-muted-foreground">
          {{ mode === 'search' ? resultCount : '' }}
        </span>
        <p class="shrink-0 text-right text-xs text-muted-foreground">
          <i18n-t keypath="icons.picker.credit" scope="global">
            <template #iconify>
              <ExternalLink href="https://iconify.design/">
                Iconify
              </ExternalLink>
            </template>
          </i18n-t><br>
          <i18n-t keypath="icons.picker.explore" scope="global">
            <template #site>
              <ExternalLink href="https://icones.js.org/">
                icones.js.org
              </ExternalLink>
            </template>
          </i18n-t>
        </p>
      </div>
    </DialogContent>
  </Dialog>
</template>
