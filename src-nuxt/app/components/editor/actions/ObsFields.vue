<script setup lang="ts">
import { open as choose } from '@tauri-apps/plugin-dialog'
import { Folder, ToggleRight, X } from '@lucide/vue'
import { commands } from '~/ipc/bindings'
import { KEY_DRAFT } from '~/lib/actions/key-draft'
import { followInKey, followedCommand, followedState, obsStatusText, sameCommand, switchLabels } from '~/lib/actions/obs'
import type { ObsCommand, ObsMediaControl, ObsSwitch } from '~/types/engine'

const command = defineModel<ObsCommand>({ required: true })
const props = defineProps<{ server: string }>()

const { t } = useI18n()
const { snapshot } = useEngine()
const draft = inject(KEY_DRAFT, null)

/** Each with its name in `actions.obs.media`. */
const MEDIA: { value: ObsMediaControl, name: string }[] = [
  { value: 'play_pause', name: 'playPause' },
  { value: 'play', name: 'play' },
  { value: 'pause', name: 'pause' },
  { value: 'stop', name: 'stop' },
  { value: 'restart', name: 'restart' },
  { value: 'next', name: 'next' },
  { value: 'previous', name: 'previous' },
]
const DEFAULT_STEP_DB = 3

// Connecting now brings OBS's names to the pickers before the key is even saved.
onMounted(() => {
  commands.obsConnect(props.server)
})

const obs = computed(() => snapshot.value?.obs[props.server])
const connected = computed(() => obs.value?.status.state === 'connected')

/** Edits some fields of the command, whatever its kind, keeping the rest. */
function patch(changes: Record<string, unknown>) {
  command.value = { ...command.value, ...changes } as ObsCommand
}

const labels = computed(() => switchLabels(command.value.kind))
const mode = computed(() => ('mode' in command.value ? command.value.mode : 'toggle'))

function setMode(value: unknown) {
  if (typeof value === 'string' && value) {
    patch({ mode: value as ObsSwitch })
  }
}

function setMedia(value: unknown) {
  if (typeof value === 'string') {
    patch({ control: value as ObsMediaControl })
  }
}

const audio = computed(() => (command.value.kind === 'mute' || command.value.kind === 'volume' ? command.value.kind : null))

function setAudio(value: unknown) {
  const current = command.value
  if ((value !== 'mute' && value !== 'volume') || value === current.kind || !('input' in current)) {
    return
  }
  command.value = value === 'mute'
    ? { kind: 'mute', input: current.input, mode: 'toggle' }
    : { kind: 'volume', input: current.input, change: { type: 'step', db: DEFAULT_STEP_DB } }
}

type VolumeWay = 'up' | 'down' | 'set'

const volumeWay = computed<VolumeWay>(() => {
  const value = command.value
  if (value.kind !== 'volume' || value.change.type === 'set') {
    return 'set'
  }
  return value.change.db < 0 ? 'down' : 'up'
})
const volumeAmount = computed(() => {
  const value = command.value
  if (value.kind !== 'volume') {
    return 0
  }
  return value.change.type === 'set' ? value.change.percent : Math.abs(value.change.db)
})

function setVolume(way: VolumeWay, amount: number) {
  const value = Number.isFinite(amount) ? Math.round(amount) : 0
  const change = way === 'set'
    ? { type: 'set', percent: Math.min(100, Math.max(0, value)) }
    : { type: 'step', db: (way === 'down' ? -1 : 1) * Math.min(30, Math.max(1, value)) }
  patch({ change })
}

function setWay(value: unknown) {
  if (value === 'up' || value === 'down' || value === 'set') {
    const amount = value === 'set' ? (volumeWay.value === 'set' ? volumeAmount.value : 80) : (volumeWay.value === 'set' ? DEFAULT_STEP_DB : volumeAmount.value)
    setVolume(value, amount)
  }
}

function setDuration(value: string | number) {
  const text = String(value).trim()
  patch({ duration_ms: text ? Math.round(Number(text)) : null })
}

async function pickFolder() {
  const chosen = await choose({ title: t('actions.obs.chooseFolder'), directory: true })
  if (typeof chosen === 'string') {
    patch({ folder: chosen })
  }
}

/** The OBS state this command can show on the key, and whether the key already follows it. */
const state = computed(() => followedState(command.value))
const followed = computed(() => (draft?.value ? followedCommand(draft.value.actions, draft.value.second?.actions ?? []) : null))
const follows = computed(() => Boolean(draft?.value?.second && followed.value && sameCommand(followed.value, command.value)))
const canFollow = computed(() => {
  const key = draft?.value
  return Boolean(state.value && key && !key.second && followed.value && sameCommand(followed.value, command.value))
})
const hasGestures = computed(() => Boolean(draft?.value?.doublePress.length || draft?.value?.longPress.length))

function follow() {
  if (draft?.value) {
    followInKey(draft.value, command.value, props.server)
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <template v-if="command.kind === 'set_transition'">
      <ObsNameField :server="props.server" :model-value="command.transition" :list="{ kind: 'transitions' }" :label="t('actions.obs.transition')" @update:model-value="patch({ transition: $event })" />
      <label class="flex items-center justify-between gap-2 text-xs">
        {{ t('actions.obs.duration') }}
        <span class="flex items-center gap-1">
          <Input
            :model-value="command.duration_ms ?? ''"
            type="number"
            min="50"
            max="20000"
            step="50"
            :placeholder="t('actions.obs.durationPlaceholder')"
            class="h-8 w-28 text-right"
            :aria-label="t('actions.obs.durationLabel')"
            @update:model-value="setDuration"
          />
          <span class="text-muted-foreground">ms</span>
        </span>
      </label>
    </template>

    <template v-else-if="command.kind === 'screenshot'">
      <ObsNameField :server="props.server" :model-value="command.source ?? ''" :list="{ kind: 'sources' }" :label="t('actions.obs.sourceOrScene')" :placeholder="t('actions.obs.programScene')" @update:model-value="patch({ source: $event })" />
      <div class="flex items-center gap-1.5">
        <div class="flex min-w-0 flex-1 items-center gap-2 rounded-md border px-3 py-1.5 text-xs" :title="command.folder || undefined">
          <Folder class="size-3.5 shrink-0 text-muted-foreground" />
          <span class="truncate" :class="command.folder ? '' : 'text-muted-foreground'">{{ command.folder || t('actions.obs.recordingsFolder') }}</span>
        </div>
        <Button v-if="command.folder" variant="ghost" size="icon-sm" :aria-label="t('actions.obs.saveToRecordings')" @click="patch({ folder: '' })">
          <X />
        </Button>
        <Button variant="outline" size="sm" @click="pickFolder">
          {{ t('actions.obs.folder') }}
        </Button>
      </div>
    </template>

    <ObsNameField
      :server="props.server"
      v-else-if="command.kind === 'scene_collection'"
      :model-value="command.collection"
      :list="{ kind: 'scene_collections' }"
      :label="t('actions.obs.sceneCollection')"
      @update:model-value="patch({ collection: $event })"
    />

    <ObsNameField :server="props.server" v-else-if="command.kind === 'profile'" :model-value="command.profile" :list="{ kind: 'profiles' }" :label="t('actions.obs.profile')" @update:model-value="patch({ profile: $event })" />

    <template v-else-if="command.kind === 'scene'">
      <ObsNameField :server="props.server" :model-value="command.scene" :list="{ kind: 'scenes' }" :label="t('actions.obs.scene')" @update:model-value="patch({ scene: $event })" />
      <label class="flex items-center justify-between gap-2 text-xs">
        <span>{{ t('actions.obs.toPreview') }} <span class="text-muted-foreground">{{ t('actions.obs.studioModeHint') }}</span></span>
        <Switch :model-value="Boolean(command.preview)" :aria-label="t('actions.obs.toPreview')" @update:model-value="patch({ preview: $event })" />
      </label>
    </template>

    <template v-else-if="command.kind === 'source_visibility'">
      <ObsNameField :server="props.server" :model-value="command.scene" :list="{ kind: 'scenes' }" :label="t('actions.obs.scene')" @update:model-value="patch({ scene: $event })" />
      <ObsNameField :server="props.server" :model-value="command.source" :list="{ kind: 'scene_items', scene: command.scene }" :label="t('actions.obs.source')" @update:model-value="patch({ source: $event })" />
    </template>

    <template v-else-if="command.kind === 'chapter'">
      <Input
        :model-value="command.name ?? ''"
        :placeholder="t('actions.obs.chapterPlaceholder')"
        :aria-label="t('actions.obs.chapterLabel')"
        @update:model-value="patch({ name: String($event) })"
      />
      <p class="text-[11px] text-muted-foreground">
        {{ t('actions.obs.chapterHint') }}
      </p>
    </template>

    <template v-else-if="command.kind === 'filter'">
      <ObsNameField :server="props.server" :model-value="command.source" :list="{ kind: 'sources' }" :label="t('actions.obs.sourceOrScene')" @update:model-value="patch({ source: $event })" />
      <ObsNameField :server="props.server" :model-value="command.filter" :list="{ kind: 'filters', source: command.source }" :label="t('actions.obs.filter')" @update:model-value="patch({ filter: $event })" />
    </template>

    <template v-else-if="command.kind === 'media'">
      <ObsNameField :server="props.server" :model-value="command.input" :list="{ kind: 'media_inputs' }" :label="t('actions.obs.mediaSource')" @update:model-value="patch({ input: $event })" />
      <Select :model-value="command.control" @update:model-value="setMedia">
        <SelectTrigger class="w-full" :aria-label="t('actions.obs.mediaControl')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="m in MEDIA" :key="m.value" :value="m.value">
            {{ t(`actions.obs.media.${m.name}`) }}
          </SelectItem>
        </SelectContent>
      </Select>
    </template>

    <template v-else-if="command.kind === 'mute' || command.kind === 'volume'">
      <ObsNameField :server="props.server" :model-value="command.input" :list="{ kind: 'audio_inputs' }" :label="t('actions.obs.audioSource')" @update:model-value="patch({ input: $event })" />
      <ToggleGroup :model-value="audio" type="single" variant="outline" size="sm" class="w-full" :aria-label="t('actions.obs.muteOrVolume')" @update:model-value="setAudio">
        <ToggleGroupItem value="mute" class="flex-1">
          {{ t('actions.obs.mute') }}
        </ToggleGroupItem>
        <ToggleGroupItem value="volume" class="flex-1">
          {{ t('actions.obs.volume') }}
        </ToggleGroupItem>
      </ToggleGroup>
      <div v-if="command.kind === 'volume'" class="flex items-center gap-2">
        <ToggleGroup :model-value="volumeWay" type="single" variant="outline" size="sm" class="flex-1" :aria-label="t('actions.obs.volumeWay')" @update:model-value="setWay">
          <ToggleGroupItem value="up" class="flex-1">
            {{ t('actions.obs.up') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="down" class="flex-1">
            {{ t('actions.obs.down') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="set" class="flex-1">
            {{ t('actions.obs.set') }}
          </ToggleGroupItem>
        </ToggleGroup>
        <Input
          :model-value="volumeAmount"
          type="number"
          :min="volumeWay === 'set' ? 0 : 1"
          :max="volumeWay === 'set' ? 100 : 30"
          class="h-8 w-20 text-right"
          :aria-label="t(volumeWay === 'set' ? 'actions.obs.percent' : 'actions.obs.decibels')"
          @update:model-value="setVolume(volumeWay, Number($event))"
        />
        <span class="w-5 text-xs text-muted-foreground">{{ volumeWay === 'set' ? '%' : 'dB' }}</span>
      </div>
      <p v-if="command.kind === 'volume'" class="text-[11px] text-muted-foreground">
        {{ t(volumeWay === 'set' ? 'actions.obs.setHint' : 'actions.obs.stepHint') }}
      </p>
    </template>

    <p v-else-if="command.kind === 'transition'" class="text-[11px] text-muted-foreground">
      {{ t('actions.obs.transitionHint') }}
    </p>
    <p v-else-if="command.kind === 'save_replay'" class="text-[11px] text-muted-foreground">
      {{ t('actions.obs.replayHint') }}
    </p>

    <ToggleGroup v-if="labels" :model-value="mode" type="single" variant="outline" size="sm" class="w-full" :aria-label="t('actions.obs.whatItDoes')" @update:model-value="setMode">
      <ToggleGroupItem value="toggle" class="flex-1">
        {{ t('actions.obs.toggle') }}
      </ToggleGroupItem>
      <ToggleGroupItem value="on" class="flex-1">
        {{ labels.on }}
      </ToggleGroupItem>
      <ToggleGroupItem value="off" class="flex-1">
        {{ labels.off }}
      </ToggleGroupItem>
    </ToggleGroup>

    <p v-if="follows && state" class="text-[11px] text-muted-foreground">
      {{ t('actions.obs.follows', { state: state.while }) }}
    </p>
    <Button
      v-else-if="canFollow"
      variant="outline"
      size="sm"
      class="justify-start"
      :disabled="hasGestures"
      :title="hasGestures ? t('actions.obs.removeGestures') : t('actions.obs.giveStates', { state: state?.while })"
      @click="follow"
    >
      <ToggleRight /> {{ t('actions.obs.showOnKey') }}
    </Button>

    <p v-if="!connected && obs" class="text-[11px] text-muted-foreground">
      {{ t('actions.obs.typeByHand', { status: obsStatusText(obs) }) }}
      <NuxtLink to="/settings?tab=integrations" class="underline hover:text-foreground">
        {{ t('actions.obs.settingsLink') }}
      </NuxtLink>
    </p>
  </div>
</template>
