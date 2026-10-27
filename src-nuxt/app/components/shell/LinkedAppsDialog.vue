<script setup lang="ts">
import { open as choose } from '@tauri-apps/plugin-dialog'
import { FolderOpen, Plus, RefreshCw, X } from '@lucide/vue'
import { APP_FILES } from '~/lib/os'
import type { AppDetection, AppRef } from '~/types/engine'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ profileId: string }>()

const { profile, owner, frontApp, followApps, link, unlink, appAt, runningApps, appDetection, setUpAppDetection } = useProfiles()
const { t } = useI18n()

const current = computed(() => profile(props.profileId))
const running = ref<AppRef[]>([])
const error = ref<string | null>(null)
const busy = ref(false)

/** The app the user came from, to link it with one click. */
const suggested = computed(() => {
  const app = frontApp.value
  return app && !current.value?.apps.some(a => a.id === app.id) ? app : null
})

const detection = ref<AppDetection>('ready')

async function loadRunning() {
  running.value = await runningApps()
}

watch(open, async (value) => {
  error.value = null
  if (value) {
    loadRunning()
    detection.value = await appDetection()
  }
}, { immediate: true })

function setUpDetection() {
  attempt(async () => {
    detection.value = await setUpAppDetection()
  })
}

async function attempt(work: () => Promise<unknown>) {
  error.value = null
  busy.value = true
  try {
    await work()
  }
  catch (err) {
    error.value = err instanceof Error && !(err instanceof EngineError) ? err.message : t('shell.linkedApps.saveFailed')
  }
  finally {
    busy.value = false
  }
}

function add(app: AppRef) {
  attempt(() => link(props.profileId, app))
}

function remove(app: AppRef) {
  attempt(() => unlink(props.profileId, app.id))
}

async function pick() {
  const path = await choose({
    title: t('shell.linkedApps.pickTitle'),
    defaultPath: APP_FILES.folder,
    filters: [{ name: t('shell.linkedApps.appsFilter'), extensions: APP_FILES.extensions }],
  })
  if (typeof path !== 'string') {
    return
  }
  await attempt(async () => {
    let app: AppRef
    try {
      app = await appAt(path)
    }
    catch {
      throw new Error(t('shell.linkedApps.notAnApp'))
    }
    await link(props.profileId, app)
  })
}

/** Where an open app is linked, if not to this profile. */
function elsewhere(app: AppRef) {
  const other = owner(app.id)
  return other && other.id !== props.profileId ? other : null
}

function linkedHere(app: AppRef) {
  return current.value?.apps.some(a => a.id === app.id) ?? false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>{{ t('shell.linkedApps.title', { name: current?.name ?? '' }) }}</DialogTitle>
        <DialogDescription>
          {{ t('shell.linkedApps.description') }}
        </DialogDescription>
      </DialogHeader>

      <p v-if="!followApps" class="rounded-md bg-amber-500/10 px-3 py-2 text-xs text-amber-300">
        {{ t('shell.linkedApps.followOff') }}
        <NuxtLink to="/settings?tab=profiles" class="underline" @click="open = false">
          {{ t('shell.linkedApps.enableInSettings') }}
        </NuxtLink>.
      </p>

      <p v-if="detection !== 'ready'" class="rounded-md bg-amber-500/10 px-3 py-2 text-xs text-amber-300" data-app-detection>
        {{ t(`shell.linkedApps.detection.${detection}`) }}
        <Button v-if="detection === 'needs_extension'" variant="link" size="sm" class="h-auto p-0 text-xs text-amber-100 underline" :disabled="busy" @click="setUpDetection">
          {{ t('shell.linkedApps.detection.install') }}
        </Button>
      </p>

      <div class="flex flex-col gap-1" role="list" :aria-label="t('shell.linkedApps.linked')">
        <div
          v-for="app in current?.apps ?? []"
          :key="app.id"
          role="listitem"
          class="flex items-center gap-2.5 rounded-md bg-neutral-900/60 px-2.5 py-1.5"
        >
          <AppIcon :app-id="app.id" />
          <div class="flex min-w-0 flex-1 flex-col">
            <span class="truncate text-sm">{{ app.name }}</span>
            <span class="truncate text-[11px] text-muted-foreground">{{ app.id }}</span>
          </div>
          <Button variant="ghost" size="icon-sm" :aria-label="t('shell.linkedApps.remove', { name: app.name })" :disabled="busy" @click="remove(app)">
            <X />
          </Button>
        </div>
        <p v-if="!current?.apps.length" class="py-2 text-center text-xs text-muted-foreground">
          {{ t('shell.linkedApps.none') }}
        </p>
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex items-center justify-between">
          <p class="text-xs font-medium text-muted-foreground">
            {{ t('shell.linkedApps.add') }}
          </p>
          <Button variant="ghost" size="icon-sm" :aria-label="t('shell.linkedApps.refresh')" :title="t('shell.linkedApps.refresh')" @click="loadRunning">
            <RefreshCw />
          </Button>
        </div>
        <Button v-if="suggested" variant="secondary" class="justify-start" :disabled="busy || Boolean(elsewhere(suggested))" @click="add(suggested)">
          <AppIcon :app-id="suggested.id" />
          <span class="truncate">{{ t('shell.linkedApps.link', { name: suggested.name }) }}</span>
          <span class="ml-auto text-[11px] text-muted-foreground">
            {{ elsewhere(suggested) ? t('shell.linkedApps.elsewhere', { name: elsewhere(suggested)!.name }) : t('shell.linkedApps.lastUsed') }}
          </span>
        </Button>
        <div class="flex max-h-48 flex-col gap-0.5 overflow-y-auto" role="list" :aria-label="t('shell.linkedApps.running')">
          <button
            v-for="app in running"
            :key="app.id"
            type="button"
            role="listitem"
            class="flex items-center gap-2.5 rounded-md px-2.5 py-1 text-left text-sm hover:bg-neutral-800/60 disabled:pointer-events-none disabled:opacity-50"
            :disabled="busy || linkedHere(app) || Boolean(elsewhere(app))"
            @click="add(app)"
          >
            <AppIcon :app-id="app.id" />
            <span class="min-w-0 flex-1 truncate">{{ app.name }}</span>
            <span v-if="linkedHere(app)" class="text-[11px] text-muted-foreground">{{ t('shell.linkedApps.linkedHere') }}</span>
            <span v-else-if="elsewhere(app)" class="truncate text-[11px] text-muted-foreground">{{ t('shell.linkedApps.elsewhere', { name: elsewhere(app)!.name }) }}</span>
            <Plus v-else class="size-3.5 text-muted-foreground" />
          </button>
        </div>
        <Button variant="outline" size="sm" class="self-start" :disabled="busy" @click="pick">
          <FolderOpen /> {{ t('shell.linkedApps.pickOther') }}
        </Button>
      </div>

      <p v-if="error" class="text-xs text-destructive">
        {{ error }}
      </p>
    </DialogContent>
  </Dialog>
</template>
