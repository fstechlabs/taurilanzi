<script setup lang="ts">
import type { DeepReadonly } from 'vue'
import { ShieldAlert } from '@lucide/vue'
import { urlError } from '~/composables/useKeyEditor'
import { entryFor } from '~/lib/actions/catalog'
import { hassError, switchLabels } from '~/lib/actions/hass'
import { obsError } from '~/lib/actions/obs'
import { NO_SERVER } from '~/lib/integrations'
import { httpError, httpSummary } from '~/lib/actions/http'
import { findAction } from '~/lib/actions/plugins'
import { os } from '~/lib/os'
import type { Action, HassCommand, MediaKey, Page } from '~/types/engine'

const action = defineModel<Action>({ required: true })
const props = defineProps<{
  page: string
  otherPages: DeepReadonly<Page>[]
  /** Just added from the catalog: an action set up in a dialog opens it straight away. */
  openOnMount?: boolean
}>()

const { t } = useI18n()
const { snapshot } = useEngine()
const hass = useHass(() => (action.value.type === 'home_assistant' ? action.value.server ?? NO_SERVER : ''))
const integrations = useIntegrations()
/** The server an action of OBS or Home Assistant talks to, if it is still in the integrations. */
const server = computed(() => (action.value.type === 'obs' || action.value.type === 'home_assistant' ? integrations.server(action.value.server) : undefined))
const accessibility = useAccessibility()

const MEDIA: MediaKey[] = ['play_pause', 'next', 'previous', 'volume_up', 'volume_down', 'mute', 'brightness_up', 'brightness_down']

function mediaName(key: MediaKey) {
  return entryFor({ type: 'media', key })?.name ?? key
}

const needsPermission = computed(() => Boolean(entryFor(action.value)?.accessibility) && !accessibility.granted.value)
const url = computed(() => (action.value.type === 'open_url' ? action.value.url : ''))
const profiles = computed(() => (snapshot.value?.profiles ?? []).filter(p => p.id !== snapshot.value?.active_profile))

/** Edits one field of the action, whatever its type, keeping the rest. */
function patch(changes: Record<string, unknown>) {
  action.value = { ...action.value, ...changes } as Action
}

function selected(value: unknown, apply: (value: string) => void) {
  if (typeof value === 'string') {
    apply(value)
  }
}

const entry = computed(() => entryFor(action.value))
/** Actions with many fields are set up in a dialog, so the list stays short. */
const configuring = ref(false)
/** Long enough for the catalog to finish closing: two dialogs at once fight over the focus. */
const AFTER_CATALOG_MS = 250
onMounted(() => {
  if (props.openOnMount && entry.value?.modal) {
    setTimeout(() => {
      configuring.value = true
    }, AFTER_CATALOG_MS)
  }
})
const { catalog } = usePlugins()
const pluginFound = computed(() => (action.value.type === 'plugin' ? findAction(catalog.value, action.value.action) : null))
const summary = computed(() => {
  const value = action.value
  if (value.type === 'http') {
    return { ...httpSummary(value.request), mono: true }
  }
  if (value.type === 'home_assistant') {
    const summary = hassSummary(value.command)
    return { ...summary, detail: [server.value?.name ?? t('integrations.errors.choose'), summary.detail].filter(Boolean).join(' · ') }
  }
  if (value.type === 'obs') {
    return { title: entry.value?.name ?? '', detail: server.value?.name ?? t('integrations.errors.choose'), mono: false }
  }
  if (value.type === 'plugin') {
    const found = pluginFound.value
    return {
      title: found ? `${found.plugin.name} · ${found.action.name}` : t('actions.details.pluginMissing'),
      detail: found ? t(found.plugin.connected ? 'actions.details.pluginSettings' : 'actions.details.pluginStopped') : value.action,
      mono: false,
    }
  }
  return { title: '', detail: '', mono: false }
})
const problem = computed(() => {
  const value = action.value
  if (value.type === 'http') {
    return httpError(value.request)
  }
  if ((value.type === 'obs' || value.type === 'home_assistant') && !server.value) {
    return t('integrations.errors.choose')
  }
  if (value.type === 'obs') {
    return obsError(value.command)
  }
  return value.type === 'home_assistant' ? hassError(value.command) : null
})

/** A command of Home Assistant in a line: what it acts on, and what it does to it. */
function hassSummary(command: HassCommand) {
  const name = (id: string) => hass.entity(id)?.name ?? id
  switch (command.kind) {
    case 'call': {
      const target = [...(command.target?.entities ?? []).map(name), ...(command.target?.areas ?? []).map(id => hass.areaName(id) ?? id)]
      return {
        title: command.action ? `${hass.serviceName(command.action)} · ${command.action}` : t('actions.hass.call.chooseAction'),
        detail: target.length ? target.join(', ') : t('actions.hass.call.noTarget'),
        mono: false,
      }
    }
    case 'assist':
      return { title: command.text ? `«${command.text}»` : t('actions.hass.assistText'), detail: t('actions.hass.summary.assist'), mono: false }
    case 'event':
      return { title: command.event || t('actions.hass.eventType'), detail: t('actions.hass.summary.event'), mono: true }
    default:
      return {
        title: command.entity ? name(command.entity) : t('actions.hass.chooseEntity'),
        detail: command.entity ? `${command.entity} · ${hassDoes(command)}` : '',
        mono: false,
      }
  }
}

function hassDoes(command: HassCommand): string {
  const camel = (text: string) => text.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
  switch (command.kind) {
    case 'switch': {
      const labels = switchLabels(command.entity)
      return !labels ? t('actions.hass.onlyRuns') : command.mode === 'toggle' ? t('actions.hass.toggle') : labels[command.mode]
    }
    case 'level': {
      const change = command.change
      const amount = change.type === 'set' ? `= ${change.to}` : `${change.by > 0 ? '+' : ''}${change.by}`
      return `${t(`actions.hass.levels.${camel(command.level)}`)} ${amount}`
    }
    case 'choose':
      return `${t(`actions.hass.options.${camel(command.option)}`)}: ${command.choice.type === 'set' ? command.choice.option : t(`actions.hass.${command.choice.type}`)}`
    case 'media':
      return t(`actions.hass.media.${camel(command.control)}`)
    case 'alarm':
      return t(`actions.hass.alarm.${camel(command.mode)}`)
    case 'color':
      return t('actions.hass.color')
    default:
      return entryFor(action.value)?.name ?? ''
  }
}

const delaySeconds = computed({
  get: () => [action.value.type === 'delay' ? action.value.ms / 1000 : 0],
  set: ([seconds]) => patch({ ms: Math.round((seconds ?? 0) * 1000) }),
})
</script>

<template>
  <div class="flex flex-col gap-2">
    <div v-if="needsPermission" class="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-200">
      <ShieldAlert class="mt-0.5 size-4 shrink-0" />
      <div class="flex-1">
        {{ t(`actions.details.accessibility.${os}`) }}
        <Button variant="link" size="sm" class="h-auto p-0 text-xs text-amber-100 underline" @click="accessibility.request()">
          {{ t('actions.details.grantPermission') }}
        </Button>
      </div>
    </div>

    <template v-if="entry?.modal">
      <button
        type="button"
        class="flex items-center gap-2 rounded-md border border-dashed px-2.5 py-2 text-left transition-colors hover:border-sky-500/60 hover:bg-sky-500/5"
        :aria-label="t('actions.details.configureLabel', { name: entry.name })"
        data-action-summary
        @click="configuring = true"
      >
        <span class="min-w-0 flex-1">
          <span class="block truncate text-xs" :class="summary.mono ? 'font-mono' : 'font-medium'">{{ summary.title }}</span>
          <span class="block truncate text-[11px] text-muted-foreground">{{ summary.detail }}</span>
        </span>
        <span class="shrink-0 text-xs text-sky-400">{{ t('actions.details.configure') }}</span>
      </button>
      <p v-if="problem" class="text-xs text-destructive">
        {{ problem }}
      </p>
      <Dialog v-model:open="configuring">
        <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl" data-action-dialog>
          <DialogHeader class="border-b px-5 pt-5 pb-4">
            <DialogTitle>{{ entry.name }}</DialogTitle>
            <DialogDescription>{{ t('actions.details.dialogDescription', { description: entry.description.replace(/[.\s]+$/, '') }) }}</DialogDescription>
          </DialogHeader>
          <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
            <HttpFields v-if="action.type === 'http'" :model-value="action.request" @update:model-value="patch({ request: $event })" />
            <PluginInspector v-else-if="action.type === 'plugin'" :action="action.action" :id="action.id" />
            <div v-else-if="action.type === 'home_assistant'" class="flex flex-col gap-4">
              <ServerField kind="home_assistant" :model-value="action.server ?? NO_SERVER" @update:model-value="patch({ server: $event })" />
              <template v-if="server">
                <HassCallFields v-if="action.command.kind === 'call'" :model-value="action.command" :server="action.server ?? NO_SERVER" @update:model-value="patch({ command: $event })" />
                <HassFields v-else :model-value="action.command" :server="action.server ?? NO_SERVER" @update:model-value="patch({ command: $event })" />
                <HassKeyLook :command="action.command" :server="action.server ?? NO_SERVER" />
              </template>
            </div>
            <div v-else-if="action.type === 'obs'" class="flex flex-col gap-4">
              <ServerField kind="obs" :model-value="action.server ?? NO_SERVER" @update:model-value="patch({ server: $event })" />
              <ObsFields v-if="server" :model-value="action.command" :server="action.server ?? NO_SERVER" @update:model-value="patch({ command: $event })" />
            </div>
          </div>
          <div class="flex items-center justify-between gap-3 rounded-b-xl border-t bg-muted/50 px-5 py-3">
            <p class="text-xs" :class="problem ? 'text-destructive' : 'text-muted-foreground'">
              {{ problem ?? t('actions.details.ready') }}
            </p>
            <Button size="sm" @click="configuring = false">
              {{ t('actions.details.done') }}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </template>

    <template v-else-if="action.type === 'open_url'">
      <Input
        :model-value="url"
        placeholder="https://…"
        :aria-label="t('actions.details.address')"
        :aria-invalid="Boolean(urlError(url))"
        @update:model-value="patch({ url: String($event) })"
      />
      <p v-if="urlError(url)" class="text-xs text-destructive">
        {{ urlError(url) }}
      </p>
    </template>

    <PathField v-else-if="action.type === 'open'" :model-value="action.path" @update:model-value="patch({ path: $event })" />
    <PathField v-else-if="action.type === 'close_app'" :model-value="action.app" apps-only @update:model-value="patch({ app: $event })" />

    <template v-else-if="action.type === 'type_text'">
      <Textarea
        :model-value="action.text"
        :placeholder="t('actions.details.textPlaceholder')"
        rows="3"
        :aria-label="t('actions.details.textLabel')"
        @update:model-value="patch({ text: String($event) })"
      />
      <label class="flex items-center justify-between gap-2 text-xs">
        {{ t('actions.details.pressEnter') }}
        <Switch :model-value="Boolean(action.enter)" :aria-label="t('actions.details.pressEnter')" @update:model-value="patch({ enter: $event })" />
      </label>
      <label class="flex items-center justify-between gap-2 text-xs">
        <span>{{ t('actions.details.paste') }} <span class="text-muted-foreground">{{ t('actions.details.pasteHint') }}</span></span>
        <Switch :model-value="Boolean(action.paste)" :aria-label="t('actions.details.paste')" @update:model-value="patch({ paste: $event })" />
      </label>
    </template>

    <HotkeyField v-else-if="action.type === 'hotkey'" :model-value="action.keys" @update:model-value="patch({ keys: $event })" />

    <Select v-else-if="action.type === 'media'" :model-value="action.key" @update:model-value="selected($event, key => patch({ key }))">
      <SelectTrigger class="w-full" :aria-label="t('actions.details.mediaKey')">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="key in MEDIA" :key="key" :value="key">
          {{ mediaName(key) }}
        </SelectItem>
      </SelectContent>
    </Select>

    <Select v-else-if="action.type === 'open_page'" :model-value="action.page" @update:model-value="selected($event, page => patch({ page, waypoints: [] }))">
      <SelectTrigger class="w-full" :aria-label="t('actions.details.targetPage')">
        <SelectValue :placeholder="t('actions.details.choosePage')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="p in props.otherPages" :key="p.id" :value="p.id">
          {{ p.name }}
        </SelectItem>
      </SelectContent>
    </Select>

    <Select v-else-if="action.type === 'switch_profile'" :model-value="action.profile" @update:model-value="selected($event, profile => patch({ profile }))">
      <SelectTrigger class="w-full" :aria-label="t('actions.details.targetProfile')">
        <SelectValue :placeholder="t('actions.details.chooseProfile')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="p in profiles" :key="p.id" :value="p.id">
          {{ p.name }}
        </SelectItem>
      </SelectContent>
    </Select>

    <div v-else-if="action.type === 'delay'" class="flex items-center gap-3">
      <Slider v-model="delaySeconds" :min="0" :max="10" :step="0.1" class="flex-1" :aria-label="t('actions.details.wait')" />
      <div class="flex items-center gap-1">
        <Input
          :model-value="action.ms / 1000"
          type="number"
          min="0"
          max="60"
          step="0.1"
          class="h-8 w-20 text-right"
          :aria-label="t('actions.details.waitSeconds')"
          @update:model-value="patch({ ms: Math.min(60000, Math.max(0, Math.round(Number($event) * 1000))) })"
        />
        <span class="text-xs text-muted-foreground">s</span>
      </div>
    </div>

    <div v-else-if="action.type === 'random'" class="rounded-md border border-dashed p-2">
      <ActionList :model-value="action.actions" :page="props.page" nested @update:model-value="patch({ actions: $event })" />
    </div>

    <SoundFields v-else-if="action.type === 'play_sound'" :model-value="action" @update:model-value="action = $event" />
  </div>
</template>
