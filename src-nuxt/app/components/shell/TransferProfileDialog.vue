<script setup lang="ts">
import { ArrowRight, Check, TriangleAlert } from '@lucide/vue'
import { commands } from '~/ipc/bindings'
import { failure } from '~/lib/errors'
import type { TransferPlan } from '~/types/engine'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ profileId: string, profileName: string }>()

const { snapshot } = useEngine()
const { t } = useI18n()

/** The docks the profile can go to: every one but its own. */
const targets = computed(() => (snapshot.value?.devices ?? []).filter(d => d.id !== snapshot.value?.device_id))
const target = ref<string | null>(null)
const plan = ref<TransferPlan | null>(null)
const working = ref(false)
const error = ref<string | null>(null)

watch(open, (visible) => {
  if (visible) {
    target.value = targets.value[0]?.id ?? null
    error.value = null
  }
}, { immediate: true })

watch([target, open], async ([device, visible]) => {
  plan.value = null
  if (!device || !visible) {
    return
  }
  try {
    plan.value = await call(commands.transferPlan(props.profileId, device))
  }
  catch (err) {
    error.value = failure(t('shell.transfer.errors.plan'), err)
  }
}, { immediate: true })

const fits = computed(() => Boolean(plan.value && !plan.value.removed.length && !plan.value.unsupported.length))

function describe(key: { page_name: string, index: number }) {
  return t('shell.transfer.key', { page: key.page_name, number: key.index + 1 })
}

/** The profile, or its copy, goes to the other dock, which the editor shows next. */
async function transfer(copy: boolean) {
  const device = target.value
  if (!device) {
    return
  }
  working.value = true
  error.value = null
  try {
    await call(commands.transferProfile(props.profileId, device, copy))
    await call(commands.focusDevice(device))
    open.value = false
  }
  catch (err) {
    error.value = failure(copy ? t('shell.transfer.errors.copy') : t('shell.transfer.errors.move'), err)
  }
  finally {
    working.value = false
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md" data-profile-transfer>
      <DialogHeader>
        <DialogTitle>{{ t('shell.transfer.title', { name: props.profileName }) }}</DialogTitle>
        <DialogDescription>{{ t('shell.transfer.hint') }}</DialogDescription>
      </DialogHeader>
      <div class="flex flex-col gap-3 text-sm">
        <Select v-model="target">
          <SelectTrigger :aria-label="t('shell.transfer.target')">
            <SelectValue :placeholder="t('shell.transfer.target')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="device in targets" :key="device.id" :value="device.id">
              {{ device.name }}
            </SelectItem>
          </SelectContent>
        </Select>
        <p v-if="fits" class="flex items-center gap-2 text-emerald-400" data-transfer-fits>
          <Check class="size-4 shrink-0" />
          {{ t('shell.transfer.fits') }}
        </p>
        <template v-else-if="plan">
          <div v-if="plan.removed.length" class="flex flex-col gap-1" data-transfer-removed>
            <p class="text-xs font-medium text-muted-foreground">
              {{ t('shell.transfer.removed') }}
            </p>
            <ul class="max-h-28 overflow-y-auto text-xs">
              <li v-for="key in plan.removed" :key="`${key.page}:${key.index}`">
                {{ describe(key) }}
              </li>
            </ul>
          </div>
          <div v-if="plan.unsupported.length" class="flex flex-col gap-1" data-transfer-unsupported>
            <p class="flex items-center gap-1.5 text-xs font-medium text-amber-300">
              <TriangleAlert class="size-3.5" />
              {{ t('shell.transfer.unsupported') }}
            </p>
            <ul class="max-h-28 overflow-y-auto text-xs">
              <li v-for="key in plan.unsupported" :key="`${key.page}:${key.index}`">
                {{ describe(key) }}
              </li>
            </ul>
          </div>
        </template>
        <p v-if="plan?.only_profile" class="text-xs text-muted-foreground" data-transfer-only>
          {{ t('shell.transfer.onlyProfile') }}
        </p>
        <p v-if="error" class="text-xs text-destructive">
          {{ error }}
        </p>
      </div>
      <DialogFooter>
        <Button variant="outline" @click="open = false">
          {{ t('shell.transfer.cancel') }}
        </Button>
        <Button variant="outline" :disabled="!plan || working" data-transfer-copy @click="transfer(true)">
          {{ t('shell.transfer.copy') }}
        </Button>
        <Button :disabled="!plan || plan.only_profile || working" data-transfer-move @click="transfer(false)">
          {{ t('shell.transfer.move') }}
          <ArrowRight class="size-4" />
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
