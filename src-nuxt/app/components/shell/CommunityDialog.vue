<script setup lang="ts">
import { ArrowUpCircle, Check, Download, ExternalLink, LoaderCircle, Puzzle, RefreshCw, Search, ShieldAlert, Star } from '@lucide/vue'
import type { DeepReadonly } from 'vue'
import { openExternal } from '~/lib/external'
import { communityImage } from '~/lib/actions/plugins'
import type { CommunityPlugin } from '~/types/engine'

type Plugin = DeepReadonly<CommunityPlugin>

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ installed: [id: string] }>()

const { plugins, loading, failure, installing, load, install } = useCommunityPlugins()
const { show } = useToasts()
const { t, locale } = useI18n()

const query = ref('')
const chosen = ref<string | null>(null)

watch(open, (value) => {
  if (value) {
    load()
  }
}, { immediate: true })

const shown = computed(() => {
  const words = query.value.toLowerCase().split(/\s+/).filter(Boolean)
  return (plugins.value ?? []).filter((p) => {
    const text = [p.name, p.author, p.description, p.category, ...p.tags].join(' ').toLowerCase()
    return words.every(w => text.includes(w))
  })
})

const selected = computed(() => shown.value.find(p => p.id === chosen.value) ?? shown.value[0] ?? null)

function downloads(count: number) {
  return t('plugins.community.downloads', { downloads: new Intl.NumberFormat(locale.value).format(count) }, count)
}

function warned(plugin: Plugin) {
  return plugin.security.status === 'findings'
}

function findings(plugin: Plugin) {
  const { critical, high, secrets } = plugin.security
  return [
    critical && t('plugins.community.critical', { n: critical }),
    high && t('plugins.community.high', { n: high }),
    secrets && t('plugins.community.secrets', { n: secrets }),
  ].filter(Boolean).join(', ') || t('plugins.community.noDetail')
}

async function add(plugin: Plugin) {
  try {
    const id = await install(plugin.id)
    show(plugin.update ? t('plugins.updated') : t('plugins.installed'), t('plugins.community.ready', { name: plugin.name }))
    emit('installed', id)
  }
  catch (err) {
    show(t('plugins.installFailed'), err instanceof Error ? err.message : String(err))
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex h-[80vh] max-h-[720px] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl" data-community-dialog>
      <DialogHeader class="border-b px-5 pt-5 pb-4">
        <DialogTitle>{{ t('plugins.community.title') }}</DialogTitle>
        <DialogDescription>
          {{ t('plugins.community.description') }}
        </DialogDescription>
      </DialogHeader>
      <div class="flex min-h-0 flex-1">
        <div class="flex w-72 shrink-0 flex-col border-r">
          <div class="flex items-center gap-1.5 p-2">
            <div class="relative flex-1">
              <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input v-model="query" :placeholder="t('plugins.community.search')" class="h-8 pl-7 text-sm" :aria-label="t('plugins.community.searchLabel')" />
            </div>
            <Button variant="ghost" size="icon" class="size-8" :aria-label="t('plugins.community.reload')" :disabled="loading" @click="load(true)">
              <RefreshCw :class="{ 'animate-spin': loading }" />
            </Button>
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            <p v-if="failure && !plugins" class="px-2 py-3 text-xs text-muted-foreground">
              {{ t('plugins.community.loadFailed') }}
            </p>
            <p v-else-if="!plugins" class="flex items-center gap-2 px-2 py-3 text-xs text-muted-foreground">
              <LoaderCircle class="size-3.5 animate-spin" /> {{ t('plugins.community.loading') }}
            </p>
            <p v-else-if="!shown.length" class="px-2 py-3 text-xs text-muted-foreground">
              {{ t('plugins.community.noMatch') }}
            </p>
            <button
              v-for="plugin in shown"
              :key="plugin.id"
              type="button"
              class="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-neutral-800/60"
              :class="{ 'bg-neutral-800': selected?.id === plugin.id }"
              :data-community-plugin="plugin.id"
              @click="chosen = plugin.id"
            >
              <img v-if="plugin.icon" :src="communityImage(plugin.icon)" alt="" class="size-8 shrink-0 rounded-md bg-neutral-900 object-contain">
              <Puzzle v-else class="size-8 shrink-0 p-1 text-muted-foreground" />
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm">
                  {{ plugin.name }}
                </p>
                <p class="truncate text-[11px] text-muted-foreground">
                  {{ plugin.author }}
                </p>
              </div>
              <ArrowUpCircle v-if="plugin.update" class="size-4 shrink-0 text-sky-400" :aria-label="t('plugins.updateAvailable')" />
              <Check v-else-if="plugin.installed" class="size-4 shrink-0 text-emerald-500" :aria-label="t('plugins.community.installed')" />
            </button>
          </div>
        </div>

        <div v-if="selected" class="min-w-0 flex-1 overflow-y-auto px-6 py-5" data-community-details>
          <div class="flex items-start gap-4">
            <img v-if="selected.icon" :src="communityImage(selected.icon)" alt="" class="size-16 shrink-0 rounded-xl bg-neutral-900 object-contain">
            <Puzzle v-else class="size-16 shrink-0 p-2 text-muted-foreground" />
            <div class="min-w-0 flex-1">
              <h3 class="truncate text-lg font-semibold">
                {{ selected.name }}
              </h3>
              <p class="text-sm text-muted-foreground">
                {{ [selected.author, t('plugins.version', { version: selected.version }), downloads(selected.downloads)].join(' · ') }}
                <span v-if="selected.stars" class="inline-flex items-center gap-0.5"> · <Star class="size-3" /> {{ selected.stars }}</span>
              </p>
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
                  {{ installing.has(selected.id) ? t('plugins.community.installing') : selected.update ? t('plugins.updateTo', { version: selected.version }) : t('plugins.community.install') }}
                </Button>
                <Badge v-else variant="outline" class="h-8 gap-1 px-3" data-community-installed>
                  <Check class="size-3.5 text-emerald-500" /> {{ t('plugins.community.installed') }}
                </Badge>
                <Button v-if="selected.source_url" variant="ghost" size="sm" @click="openExternal(selected.source_url)">
                  <ExternalLink /> {{ t('plugins.community.source') }}
                </Button>
              </div>
            </div>
          </div>

          <div v-if="warned(selected)" class="mt-4 flex gap-2.5 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-200">
            <ShieldAlert class="size-4 shrink-0" />
            <p>
              {{ t('plugins.community.warning', { findings: findings(selected) }) }}
            </p>
          </div>

          <p class="mt-4 text-sm">
            {{ selected.description }}
          </p>
          <p v-if="selected.long_description && selected.long_description !== selected.description" class="mt-2 text-sm whitespace-pre-line text-muted-foreground">
            {{ selected.long_description }}
          </p>
          <p v-if="selected.dials" class="mt-2 text-xs text-muted-foreground">
            {{ t('plugins.community.dials') }}
          </p>

          <div v-if="selected.screenshots.length" class="mt-4 flex gap-2 overflow-x-auto pb-1">
            <img
              v-for="shot in selected.screenshots"
              :key="shot"
              :src="communityImage(shot)"
              alt=""
              class="h-44 shrink-0 rounded-md border bg-neutral-900 object-contain"
            >
          </div>

          <div v-if="selected.tags.length || selected.category" class="mt-4 flex flex-wrap gap-1.5">
            <Badge v-if="selected.category" variant="secondary">
              {{ selected.category }}
            </Badge>
            <Badge v-for="tag in selected.tags" :key="tag" variant="outline">
              {{ tag }}
            </Badge>
          </div>
        </div>
        <div v-else class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          <Puzzle class="mr-2 size-4" /> {{ t('plugins.community.choose') }}
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
