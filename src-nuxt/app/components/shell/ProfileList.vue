<script setup lang="ts">
import { AppWindow, Cloud, CloudOff, Copy, Ellipsis, FileStack, MonitorPlay, Pencil, Pin, Plus, Trash2 } from '@lucide/vue'
import type { DeepReadonly, Ref } from 'vue'
import type { ContextMenuItem } from '~/composables/useContextMenu'
import type { ProfileSummary } from '~/types/engine'

type Profile = DeepReadonly<ProfileSummary>

/** More would crowd the name out of the sidebar; the tooltip lists them all. */
const MAX_ICONS = 2

const { profiles, active, chosenBy, activate, create, rename, duplicate, remove, setLocal, moveTo } = useProfiles()
const cloud = useCloud()
const contextMenu = useContextMenu()
const { t } = useI18n()
const route = useRoute()

const error = ref<string | null>(null)
/** The profile being named: a new one when `id` is null. */
const naming = ref<{ id: string | null, name: string } | null>(null)
const linking = ref<string | null>(null)
/** A synced profile about to be deleted, or kept on this computer only: the user says what happens elsewhere. */
const deleting = ref<Profile | null>(null)
const makingLocal = ref<Profile | null>(null)

function askingOpen(target: Ref<Profile | null>) {
  return computed({
    get: () => target.value !== null,
    set: (value: boolean) => {
      if (!value) {
        target.value = null
      }
    },
  })
}
const deletingOpen = askingOpen(deleting)
const makingLocalOpen = askingOpen(makingLocal)

/** Local profiles always say so; the others, once this computer is connected, whether the cloud has their last changes. */
function cloudMark(profile: Profile) {
  if (profile.local) {
    return { state: 'local', color: 'text-neutral-500', text: t('shell.profiles.localOnly') }
  }
  if (!cloud.signedIn.value) {
    return null
  }
  return cloud.status.value?.pending.includes(profile.id)
    ? { state: 'pending', color: 'text-amber-500', text: t('shell.profiles.notSynced') }
    : { state: 'synced', color: 'text-emerald-500', text: t('shell.profiles.synced') }
}

const marks = computed(() => new Map(profiles.value.map(profile => [profile.id, cloudMark(profile)])))

/** Asks only when other computers would notice. */
function startDelete(profile: Profile) {
  if (cloud.signedIn.value && !profile.local) {
    deleting.value = profile
  }
  else {
    attempt(() => remove(profile.id), t('shell.profiles.errors.delete'))
  }
}

function startLocal(profile: Profile) {
  if (profile.local) {
    attempt(() => setLocal(profile.id, false), t('shell.profiles.errors.sync'))
  }
  else if (cloud.signedIn.value) {
    makingLocal.value = profile
  }
  else {
    attempt(() => setLocal(profile.id, true), t('shell.profiles.errors.sync'))
  }
}

function confirmDelete(everywhere: boolean) {
  const profile = deleting.value
  deleting.value = null
  if (profile) {
    attempt(() => remove(profile.id, everywhere), t('shell.profiles.errors.delete'))
  }
}

function confirmLocal(keepInCloud: boolean) {
  const profile = makingLocal.value
  makingLocal.value = null
  if (profile) {
    attempt(() => setLocal(profile.id, true, keepInCloud), t('shell.profiles.errors.sync'))
  }
}
const linkingOpen = computed({
  get: () => linking.value !== null,
  set: (value) => {
    if (!value) {
      linking.value = null
    }
  },
})

async function attempt(work: () => Promise<unknown>, message: string) {
  error.value = null
  try {
    await work()
  }
  catch {
    error.value = message
  }
}

function freeName() {
  for (let n = profiles.value.length + 1; ; n++) {
    const name = t('shell.profiles.defaultName', { n })
    if (!profiles.value.some(p => p.name === name)) {
      return name
    }
  }
}

function startCreate() {
  naming.value = { id: null, name: freeName() }
}

async function saveName() {
  const target = naming.value
  if (!target || !target.name.trim()) {
    return
  }
  naming.value = null
  if (target.id) {
    await attempt(() => rename(target.id!, target.name), t('shell.profiles.errors.rename'))
  }
  else {
    await attempt(() => create(target.name), t('shell.profiles.errors.create'))
  }
}

function items(profile: Profile): ContextMenuItem[] {
  return [
    profile.id === active.value && chosenBy.value
      ? { label: t('shell.profiles.menu.keep'), icon: Pin, run: () => attempt(() => activate(profile.id), t('shell.profiles.errors.pin')) }
      : { label: t('shell.profiles.menu.use'), icon: MonitorPlay, disabled: profile.id === active.value, run: () => attempt(() => activate(profile.id), t('shell.profiles.errors.switch')) },
    { label: t('shell.profiles.menu.rename'), icon: Pencil, separated: true, run: () => { naming.value = { id: profile.id, name: profile.name } } },
    { label: t('shell.profiles.menu.duplicate'), icon: Copy, run: () => attempt(() => duplicate(profile.id), t('shell.profiles.errors.duplicate')) },
    { label: t('shell.profiles.menu.apps'), icon: AppWindow, run: () => { linking.value = profile.id } },
    profile.local
      ? { label: t('shell.profiles.menu.sync'), icon: Cloud, run: () => startLocal(profile) }
      : { label: t('shell.profiles.menu.local'), icon: CloudOff, run: () => startLocal(profile) },
    { label: t('shell.profiles.menu.delete'), icon: Trash2, destructive: true, separated: true, disabled: profiles.value.length < 2, run: () => startDelete(profile) },
  ]
}

function menu(event: MouseEvent, profile: Profile) {
  contextMenu.open(event, items(profile))
}

async function pick(profile: Profile) {
  if (route.path !== '/') {
    await navigateTo('/')
  }
  if (profile.id !== active.value) {
    attempt(() => activate(profile.id), t('shell.profiles.errors.switch'))
  }
}

function isChooser(profile: Profile, appId: string) {
  return profile.id === active.value && chosenBy.value?.id === appId
}

/** The app that chose the profile always shows, in place of the last one if it is further down. */
function shownApps(profile: Profile) {
  const shown = profile.apps.slice(0, MAX_ICONS)
  const chooser = profile.apps.find(app => isChooser(profile, app.id))
  if (chooser && !shown.includes(chooser)) {
    shown[shown.length - 1] = chooser
  }
  return shown
}

/** The order just dropped, shown until the engine sends it back, so nothing jumps meanwhile. */
const dropped = ref<Profile[] | null>(null)
const shown = computed(() => dropped.value ?? profiles.value)
watch(profiles, () => {
  dropped.value = null
})

const sortable = useSortable(async (_, from, to) => {
  const order = [...shown.value]
  const profile = order[from]
  if (!profile) {
    return
  }
  moveItem(order, from, to)
  dropped.value = order
  await attempt(() => moveTo(profile.id, to), t('shell.profiles.errors.move'))
  dropped.value = null
})

function appsTitle(profile: Profile) {
  return t('shell.profiles.linkedApps', { apps: profile.apps.map(a => a.name).join(', ') })
}
</script>

<template>
  <div class="flex flex-col" role="list" :aria-label="t('shell.profiles.list')" data-sort-list>
    <div v-for="(profile, index) in shown" :key="profile.id" role="listitem" data-sort-item :style="sortable.itemStyle(index)">
      <div
        class="group flex cursor-default items-center gap-1.5 rounded px-2 py-1 select-none"
        :class="[
          profile.id === active ? 'bg-neutral-800 text-foreground' : 'text-muted-foreground hover:bg-neutral-800/60 hover:text-foreground',
          sortable.isDragging(index) ? 'bg-neutral-800 text-foreground shadow-lg ring-1 shadow-black/40 ring-sky-500' : '',
        ]"
        :data-profile="profile.name"
        :aria-current="profile.id === active ? 'true' : undefined"
        :title="profile.id !== active ? t('shell.profiles.use', { name: profile.name }) : chosenBy ? t('shell.profiles.chosenBy', { app: chosenBy.name }) : undefined"
        @pointerdown="sortable.press($event, index)"
        @click="sortable.wasDrag() || pick(profile)"
        @contextmenu="menu($event, profile)"
      >
        <FileStack class="size-4 shrink-0" />
        <span class="min-w-0 flex-1 truncate">{{ profile.name }}</span>
        <span v-if="profile.apps.length" :title="appsTitle(profile)" :aria-label="appsTitle(profile)" class="flex shrink-0 items-center gap-0.5">
          <span
            v-for="app in shownApps(profile)"
            :key="app.id"
            class="rounded p-0.5"
            :class="isChooser(profile, app.id) ? 'bg-emerald-500/30 ring-1 ring-emerald-400/60' : ''"
          >
            <AppIcon :app-id="app.id" small />
          </span>
        </span>
        <button
          type="button"
          class="-my-1 grid size-5 shrink-0 place-items-center rounded text-muted-foreground hover:bg-neutral-700/60 hover:text-foreground"
          :class="marks.get(profile.id) ? '' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'"
          :title="marks.get(profile.id)?.text"
          :aria-label="t('shell.profiles.options', { name: profile.name })"
          @click.stop="menu($event, profile)"
        >
          <span v-if="marks.get(profile.id)" class="group-hover:hidden" :class="marks.get(profile.id)!.color" :data-profile-cloud="marks.get(profile.id)!.state">
            <CloudOff v-if="profile.local" class="size-3.5" />
            <Cloud v-else class="size-3.5" />
          </span>
          <Ellipsis class="size-3.5" :class="marks.get(profile.id) ? 'hidden group-hover:block' : ''" />
        </button>
      </div>
    </div>
    <button
      type="button"
      class="flex items-center gap-1.5 rounded px-2 py-1 text-left text-muted-foreground hover:bg-neutral-800/60 hover:text-foreground"
      @click="startCreate"
    >
      <Plus class="size-4" />
      <span class="flex-1">{{ t('shell.profiles.new') }}</span>
    </button>
    <p v-if="error" class="px-2 py-1 text-xs text-destructive">
      {{ error }}
    </p>

    <Dialog :open="Boolean(naming)" @update:open="v => v || (naming = null)">
      <DialogContent class="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ naming?.id ? t('shell.profiles.rename') : t('shell.profiles.new') }}</DialogTitle>
          <DialogDescription v-if="!naming?.id">
            {{ t('shell.profiles.newHint') }}
          </DialogDescription>
        </DialogHeader>
        <form v-if="naming" class="flex flex-col gap-4" @submit.prevent="saveName">
          <Input v-model="naming.name" :aria-label="t('shell.profiles.nameLabel')" autofocus maxlength="60" />
          <DialogFooter>
            <Button type="button" variant="outline" @click="naming = null">
              {{ t('shell.profiles.cancel') }}
            </Button>
            <Button type="submit" :disabled="!naming.name.trim()">
              {{ naming.id ? t('shell.profiles.save') : t('shell.profiles.create') }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <LinkedAppsDialog v-if="linking" v-model:open="linkingOpen" :profile-id="linking" />

    <Dialog v-model:open="deletingOpen">
      <DialogContent class="sm:max-w-md" data-profile-delete>
        <DialogHeader>
          <DialogTitle>{{ t('shell.profiles.deleteTitle', { name: deleting?.name ?? '' }) }}</DialogTitle>
          <DialogDescription>{{ t('shell.profiles.deleteBody') }}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" @click="deleting = null">
            {{ t('shell.profiles.cancel') }}
          </Button>
          <Button variant="outline" @click="confirmDelete(false)">
            {{ t('shell.profiles.deleteHere') }}
          </Button>
          <Button variant="destructive" @click="confirmDelete(true)">
            {{ t('shell.profiles.deleteEverywhere') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <Dialog v-model:open="makingLocalOpen">
      <DialogContent class="sm:max-w-md" data-profile-local-dialog>
        <DialogHeader>
          <DialogTitle>{{ t('shell.profiles.localTitle', { name: makingLocal?.name ?? '' }) }}</DialogTitle>
          <DialogDescription>{{ t('shell.profiles.localBody') }}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" @click="makingLocal = null">
            {{ t('shell.profiles.cancel') }}
          </Button>
          <Button variant="outline" @click="confirmLocal(true)">
            {{ t('shell.profiles.localKeepCloud') }}
          </Button>
          <Button variant="destructive" @click="confirmLocal(false)">
            {{ t('shell.profiles.localRemoveCloud') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
