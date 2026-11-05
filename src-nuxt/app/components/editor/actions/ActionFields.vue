<script setup lang="ts">
import type { DeepReadonly } from 'vue'
import { ShieldAlert } from '@lucide/vue'
import { urlError } from '~/composables/useKeyEditor'
import { entryFor } from '~/lib/actions/catalog'
import { KEY_DRAFT } from '~/lib/actions/key-draft'
import { NO_SERVER } from '~/lib/integrations'
import { os } from '~/lib/os'
import type { Action, MediaKey, Page } from '~/types/engine'

const action = defineModel<Action>({ required: true })
const props = defineProps<{
  page: string
  otherPages: DeepReadonly<Page>[]
  /** What the key is, rather than one more of its actions: it may set up how the key looks. */
  face?: boolean
}>()

const { t } = useI18n()
const { snapshot } = useEngine()
// One more action of the key does not change how the key looks: its editors do not see the key.
if (!props.face) {
  provide(KEY_DRAFT, ref(null))
}
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
const typedUrl = ref(false)
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

    <HttpFields v-if="action.type === 'http'" :model-value="action.request" @update:model-value="patch({ request: $event })" />
    <PluginInspector v-else-if="action.type === 'plugin'" :action="action.action" :id="action.id" :settings="action.settings" />
    <div v-else-if="action.type === 'home_assistant'" class="flex flex-col gap-4">
      <ServerField kind="home_assistant" :model-value="action.server ?? NO_SERVER" @update:model-value="patch({ server: $event })" />
      <template v-if="server">
        <HassCallFields v-if="action.command.kind === 'call'" :model-value="action.command" :server="action.server ?? NO_SERVER" @update:model-value="patch({ command: $event })" />
        <HassFields v-else :model-value="action.command" :server="action.server ?? NO_SERVER" @update:model-value="patch({ command: $event })" />
        <HassKeyLook v-if="props.face" :command="action.command" :server="action.server ?? NO_SERVER" />
      </template>
    </div>
    <div v-else-if="action.type === 'obs'" class="flex flex-col gap-4">
      <ServerField kind="obs" :model-value="action.server ?? NO_SERVER" @update:model-value="patch({ server: $event })" />
      <ObsFields v-if="server" :model-value="action.command" :server="action.server ?? NO_SERVER" @update:model-value="patch({ command: $event })" />
    </div>

    <template v-else-if="action.type === 'open_url'">
      <Input
        :model-value="url"
        placeholder="https://…"
        autofocus
        :aria-label="t('actions.details.address')"
        :aria-invalid="typedUrl && Boolean(urlError(url))"
        @update:model-value="patch({ url: String($event) })"
        @blur="typedUrl = true"
      />
      <p v-if="typedUrl && urlError(url)" class="text-xs text-destructive">
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
      <ActionList :model-value="action.actions" :page="props.page" mode="random" @update:model-value="patch({ actions: $event })" />
    </div>

    <SoundFields v-else-if="action.type === 'play_sound'" :model-value="action" @update:model-value="action = $event" />
  </div>
</template>
