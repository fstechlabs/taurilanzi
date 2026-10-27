<script setup lang="ts">
import { ExternalLink, Plug, Trash2 } from '@lucide/vue'
import { openExternal } from '~/lib/external'
import { os } from '~/lib/os'
import type { Integration } from '~/types/engine'
import type { IntegrationKind } from '~/composables/useIntegrations'

/** A server of OBS or of Home Assistant: its name, where it is and its password or token. The
 * same dialog adds one from the settings or from the editor of an action. */
const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{
  kind: IntegrationKind
  /** The server to edit; a new one when missing. */
  server?: Integration | null
}>()
const emit = defineEmits<{ saved: [id: string] }>()

const { t } = useI18n()
const integrations = useIntegrations()

const REMOTE_EXAMPLE = 'event: taurilanzi\nevent_data:\n  page: Cámaras\n  wake: true'

const draft = ref<Integration>(blank())
const secret = ref('')
const saving = ref(false)
const failed = ref<string | null>(null)
const confirming = ref(false)
/** Saved at least once while the dialog is open, so it has a status to show. */
const saved = ref(false)

function blank(): Integration {
  const count = integrations.servers(props.kind).length
  const name = props.kind === 'obs' ? 'OBS' : 'Home Assistant'
  return props.kind === 'obs'
    ? { kind: 'obs', id: crypto.randomUUID(), name: count ? `${name} ${count + 1}` : name, host: '127.0.0.1', port: 4455 }
    : { kind: 'home_assistant', id: crypto.randomUUID(), name: count ? `${name} ${count + 1}` : name, url: '', insecure: false }
}

watch(open, (now) => {
  if (now) {
    draft.value = props.server ? structuredClone(toRaw(props.server)) as Integration : blank()
    secret.value = ''
    failed.value = null
    confirming.value = false
    saved.value = Boolean(props.server)
  }
}, { immediate: true })

const hassDraft = computed(() => (draft.value.kind === 'home_assistant' ? draft.value : null))
const obsDraft = computed(() => (draft.value.kind === 'obs' ? draft.value : null))

const isNew = computed(() => !integrations.server(draft.value.id))
const status = computed(() => (saved.value ? integrations.status(draft.value.id) : null))
const problem = computed(() => {
  const value = draft.value
  if (!value.name.trim()) {
    return t('integrations.errors.name')
  }
  if (value.kind === 'home_assistant' && !value.url.trim()) {
    return t('integrations.errors.url')
  }
  return null
})

const dot = computed(() => ({
  connected: 'bg-emerald-500',
  failed: 'bg-red-500',
  connecting: 'bg-amber-500',
  off: 'bg-neutral-500',
}[status.value?.state ?? 'off']))

/** Home Assistant's page for tokens, which the browser can only open over https. */
const tokensPage = computed(() => {
  const value = draft.value
  if (value.kind !== 'home_assistant') {
    return null
  }
  const url = value.url.trim().replace(/\/+$/, '')
  return /^https:\/\/[^/\s]+$/.test(url) ? `${url}/profile/security` : null
})

function setUrl(value: string | number) {
  if (draft.value.kind === 'home_assistant') {
    draft.value.url = String(value)
  }
}

function setInsecure(value: boolean) {
  if (draft.value.kind === 'home_assistant') {
    draft.value.insecure = value
  }
}

function setPort(value: string | number) {
  if (draft.value.kind === 'obs') {
    const port = Math.round(Number(value))
    draft.value.port = Number.isFinite(port) ? Math.min(65535, Math.max(0, port)) : 0
  }
}

async function save() {
  if (problem.value) {
    return
  }
  saving.value = true
  failed.value = null
  const value = { ...draft.value, name: draft.value.name.trim() } as Integration
  try {
    await integrations.save(value)
    if (secret.value) {
      await integrations.saveSecret(value, secret.value)
      secret.value = ''
    }
    integrations.connect(value)
    saved.value = true
    emit('saved', value.id)
  }
  catch (err) {
    failed.value = err instanceof Error ? err.message : String(err)
  }
  finally {
    saving.value = false
  }
}

async function removeSecret() {
  saving.value = true
  try {
    await integrations.saveSecret(draft.value, null)
  }
  catch (err) {
    failed.value = err instanceof Error ? err.message : String(err)
  }
  finally {
    saving.value = false
  }
}

async function remove() {
  if (!confirming.value) {
    confirming.value = true
    return
  }
  await integrations.remove(draft.value.id)
  open.value = false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg" data-integration-dialog>
      <DialogHeader class="border-b px-5 pt-5 pb-4">
        <DialogTitle>{{ t(isNew ? `integrations.new.${props.kind === 'obs' ? 'obs' : 'hass'}` : `integrations.edit.${props.kind === 'obs' ? 'obs' : 'hass'}`) }}</DialogTitle>
        <DialogDescription>{{ t(`integrations.about.${props.kind === 'obs' ? 'obs' : 'hass'}`) }}</DialogDescription>
      </DialogHeader>

      <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4">
        <div v-if="status" class="flex items-center justify-between gap-2 rounded-md border px-3 py-2">
          <span class="flex min-w-0 items-center gap-2 text-xs" data-integration-status aria-live="polite">
            <span class="size-2 shrink-0 rounded-full" :class="dot" />
            <span class="min-w-0">
              {{ status.text }}
              <span v-if="status.detail" class="block truncate text-[11px] text-muted-foreground" :title="status.detail">{{ status.detail }}</span>
            </span>
          </span>
          <Button variant="outline" size="sm" @click="integrations.connect(draft)">
            <Plug /> {{ t('integrations.connect') }}
          </Button>
        </div>

        <div class="flex flex-col gap-1.5">
          <Label class="text-xs text-muted-foreground">{{ t('integrations.name') }}</Label>
          <Input v-model="draft.name" maxlength="40" :aria-label="t('integrations.name')" />
          <p class="text-[11px] text-muted-foreground">
            {{ t('integrations.nameHint') }}
          </p>
        </div>

        <template v-if="obsDraft">
          <div class="flex gap-2">
            <div class="flex flex-1 flex-col gap-1.5">
              <Label class="text-xs text-muted-foreground">{{ t('integrations.obs.host') }}</Label>
              <Input v-model="obsDraft.host" placeholder="127.0.0.1" :aria-label="t('integrations.obs.host')" />
            </div>
            <div class="flex w-28 flex-col gap-1.5">
              <Label class="text-xs text-muted-foreground">{{ t('integrations.obs.port') }}</Label>
              <Input :model-value="obsDraft.port" type="number" min="1" max="65535" placeholder="4455" class="text-right" :aria-label="t('integrations.obs.port')" @update:model-value="setPort" />
            </div>
          </div>
          <p class="text-[11px] text-muted-foreground">
            {{ t('integrations.obs.hostHint') }}
          </p>
        </template>

        <template v-else-if="hassDraft">
          <div class="flex flex-col gap-1.5">
            <Label class="text-xs text-muted-foreground">{{ t('integrations.hass.url') }}</Label>
            <Input :model-value="hassDraft?.url ?? ''" placeholder="http://homeassistant.local:8123" :aria-label="t('integrations.hass.url')" @update:model-value="setUrl" />
            <p class="text-[11px] text-muted-foreground">
              {{ t('integrations.hass.urlHint') }}
            </p>
          </div>
          <label class="flex items-center justify-between gap-2 text-sm">
            <span>{{ t('integrations.hass.insecure') }} <span class="block text-[11px] text-muted-foreground">{{ t('integrations.hass.insecureHint') }}</span></span>
            <Switch :model-value="Boolean(hassDraft?.insecure)" @update:model-value="setInsecure" :aria-label="t('integrations.hass.insecure')" />
          </label>
        </template>

        <div class="flex flex-col gap-1.5">
          <Label class="text-xs text-muted-foreground">{{ t(draft.kind === 'obs' ? 'integrations.obs.password' : 'integrations.hass.token') }}</Label>
          <div class="flex items-center gap-1.5">
            <Input
              v-model="secret"
              type="password"
              autocomplete="off"
              class="flex-1"
              :placeholder="status?.secretSet ? '••••••••' : t(draft.kind === 'obs' ? 'integrations.obs.noPassword' : 'integrations.hass.noToken')"
              :aria-label="t(draft.kind === 'obs' ? 'integrations.obs.password' : 'integrations.hass.token')"
              @keydown.enter="save"
            />
            <Button
              v-if="status?.secretSet"
              variant="ghost"
              size="icon-sm"
              :disabled="saving"
              :aria-label="t('integrations.removeSecret')"
              :title="t('integrations.removeSecret')"
              @click="removeSecret"
            >
              <Trash2 />
            </Button>
          </div>
          <p class="text-[11px] text-muted-foreground">
            {{ status?.secretSet ? t(`integrations.secretSaved.${os}`) : t(draft.kind === 'obs' ? 'integrations.obs.passwordHint' : 'integrations.hass.tokenHint') }}
          </p>
        </div>

        <div class="flex flex-col gap-2 text-xs text-muted-foreground">
          <p>{{ t(draft.kind === 'obs' ? 'integrations.obs.howTo' : 'integrations.hass.howTo') }}</p>
          <Button v-if="tokensPage" variant="link" size="sm" class="h-auto justify-start p-0 text-xs" @click="openExternal(tokensPage)">
            <ExternalLink /> {{ t('integrations.hass.openTokens') }}
          </Button>
          <template v-if="draft.kind === 'home_assistant'">
            <p class="pt-1">
              {{ t('integrations.hass.remote') }}
            </p>
            <pre class="rounded-md bg-neutral-900 p-2 font-mono text-[11px] text-neutral-300">{{ REMOTE_EXAMPLE }}</pre>
          </template>
        </div>
      </div>

      <div class="flex items-center justify-between gap-3 rounded-b-xl border-t bg-muted/50 px-5 py-3">
        <Button v-if="!isNew" variant="ghost" size="sm" class="text-destructive" @click="remove">
          <Trash2 /> {{ confirming ? t('integrations.confirmRemove', { name: draft.name }) : t('integrations.remove') }}
        </Button>
        <p v-else class="text-xs" :class="failed || problem ? 'text-destructive' : 'text-muted-foreground'">
          {{ failed ?? problem ?? '' }}
        </p>
        <div class="flex gap-2">
          <Button variant="outline" size="sm" @click="open = false">
            {{ t('integrations.close') }}
          </Button>
          <Button size="sm" :disabled="saving || Boolean(problem)" @click="save">
            {{ t('integrations.save') }}
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
