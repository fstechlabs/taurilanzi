<script setup lang="ts">
import { failure } from '~/lib/errors'
import { ChevronDown, ChevronRight, Ellipsis, MonitorSmartphone, Pencil, Trash2, TriangleAlert } from '@lucide/vue'
import { useLocalStorage } from '@vueuse/core'
import { commands } from '~/ipc/bindings'
import type { ContextMenuItem } from '~/composables/useContextMenu'
import type { DeviceSummary } from '~/types/engine'
import { openExternal } from '~/lib/external'

/** Where users report a dock that does not work as it should. */
const ISSUES = 'https://github.com/fstechlabs/taurilanzi/issues'

const { snapshot, modelOf } = useEngine()
const contextMenu = useContextMenu()
const route = useRoute()
const { t } = useI18n()

const devices = computed(() => snapshot.value?.devices ?? [])
const error = ref<string | null>(null)
const naming = ref<{ device: DeviceSummary, name: string } | null>(null)
const forgetting = ref<DeviceSummary | null>(null)
/** Models whose warning the user already read. */
const dismissed = useLocalStorage<string[]>('untested-docks-read', [])

async function attempt(work: () => Promise<unknown>, message: string) {
  error.value = null
  try {
    await work()
  }
  catch (err) {
    error.value = failure(message, err)
  }
}

async function focus(device: DeviceSummary) {
  if (route.path !== '/') {
    await navigateTo('/')
  }
  if (device.id !== snapshot.value?.device_id) {
    attempt(() => call(commands.focusDevice(device.id)), t('shell.docks.errors.focus'))
  }
}

/** Connected docks of a model nobody has tried on the real hardware yet warn about it. */
function untested(device: DeviceSummary) {
  const model = modelOf(device.model)
  return Boolean(device.connected && model && !model.confirmed_on_hardware && !dismissed.value.includes(model.id))
}

function items(device: DeviceSummary): ContextMenuItem[] {
  return [
    { label: t('shell.docks.rename'), icon: Pencil, run: () => { naming.value = { device, name: device.custom_name } } },
    {
      label: t('shell.docks.forget'),
      icon: Trash2,
      destructive: true,
      separated: true,
      disabled: device.connected || devices.value.length < 2,
      run: () => { forgetting.value = device },
    },
  ]
}

async function saveName() {
  const target = naming.value
  if (!target) {
    return
  }
  naming.value = null
  await attempt(() => call(commands.renameDevice(target.device.id, target.name.trim())), t('shell.docks.errors.rename'))
}

async function forget() {
  const device = forgetting.value
  forgetting.value = null
  if (device) {
    await attempt(() => call(commands.forgetDevice(device.id)), t('shell.docks.errors.forget'))
  }
}

const forgettingOpen = computed({
  get: () => forgetting.value !== null,
  set: (open: boolean) => {
    if (!open) {
      forgetting.value = null
    }
  },
})
</script>

<template>
  <div class="flex flex-col" role="list" :aria-label="t('shell.sidebar.devices')">
    <div v-for="device in devices" :key="device.id" class="mb-1" role="listitem" :data-dock="device.name">
      <div
        class="group flex cursor-default items-center gap-1.5 rounded px-2 py-1 select-none"
        :class="device.id === snapshot?.device_id ? 'text-foreground' : 'text-muted-foreground hover:bg-neutral-800/60 hover:text-foreground'"
        :aria-current="device.id === snapshot?.device_id ? 'true' : undefined"
        @click="focus(device)"
        @contextmenu="contextMenu.open($event, items(device))"
      >
        <ChevronDown v-if="device.id === snapshot?.device_id" class="size-3.5 shrink-0" />
        <ChevronRight v-else class="size-3.5 shrink-0" />
        <MonitorSmartphone class="size-4 shrink-0" />
        <span class="min-w-0 flex-1 truncate" :title="device.name">{{ device.name }}</span>
        <span
          class="size-1.5 shrink-0 rounded-full"
          :class="device.connected ? 'bg-emerald-500' : 'bg-neutral-500'"
          :title="device.connected ? t('shell.sidebar.connected') : t('shell.sidebar.disconnected')"
          :data-dock-connected="device.connected"
        />
        <button
          type="button"
          class="-my-1 grid size-5 shrink-0 place-items-center rounded text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-neutral-700/60 hover:text-foreground focus-visible:opacity-100"
          :aria-label="t('shell.docks.options', { name: device.name })"
          @click.stop="contextMenu.open($event, items(device))"
        >
          <Ellipsis class="size-3.5" />
        </button>
      </div>
      <div
        v-if="untested(device)"
        class="mx-2 my-1 flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-200"
        data-dock-untested
      >
        <TriangleAlert class="mt-0.5 size-3.5 shrink-0" />
        <div class="flex flex-col gap-1">
          <p>{{ t('shell.docks.untested') }}</p>
          <div class="flex gap-3">
            <button type="button" class="underline" @click="openExternal(ISSUES)">
              {{ t('shell.docks.report') }}
            </button>
            <button type="button" class="underline" @click="dismissed = [...dismissed, device.model]">
              {{ t('shell.docks.understood') }}
            </button>
          </div>
        </div>
      </div>
      <ProfileList v-if="device.id === snapshot?.device_id" class="ml-4 border-l border-neutral-800 pl-2" />
    </div>
    <p v-if="error" class="px-2 py-1 text-xs text-destructive">
      {{ error }}
    </p>

    <Dialog :open="Boolean(naming)" @update:open="v => v || (naming = null)">
      <DialogContent class="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ t('shell.docks.renameTitle') }}</DialogTitle>
          <DialogDescription>{{ t('shell.docks.renameHint') }}</DialogDescription>
        </DialogHeader>
        <dl v-if="naming" class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-md bg-neutral-900/60 px-3 py-2 text-xs" data-dock-identity>
          <dt class="text-muted-foreground">
            {{ t('shell.docks.model') }}
          </dt>
          <dd>{{ modelOf(naming.device.model)?.name ?? naming.device.model }}</dd>
          <dt class="text-muted-foreground">
            {{ t('shell.docks.serial') }}
          </dt>
          <dd class="font-mono select-text">
            {{ naming.device.serial ?? t('shell.docks.noSerial') }}
          </dd>
        </dl>
        <form v-if="naming" class="flex flex-col gap-4" @submit.prevent="saveName">
          <Input v-model="naming.name" :aria-label="t('shell.docks.nameLabel')" autofocus maxlength="60" />
          <DialogFooter>
            <Button type="button" variant="outline" @click="naming = null">
              {{ t('shell.docks.cancel') }}
            </Button>
            <Button type="submit">
              {{ t('shell.docks.save') }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <Dialog v-model:open="forgettingOpen">
      <DialogContent class="sm:max-w-md" data-dock-forget>
        <DialogHeader>
          <DialogTitle>{{ t('shell.docks.forgetTitle', { name: forgetting?.name ?? '' }) }}</DialogTitle>
          <DialogDescription>{{ t('shell.docks.forgetBody') }}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" @click="forgetting = null">
            {{ t('shell.docks.cancel') }}
          </Button>
          <Button variant="destructive" @click="forget">
            {{ t('shell.docks.forgetConfirm') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
