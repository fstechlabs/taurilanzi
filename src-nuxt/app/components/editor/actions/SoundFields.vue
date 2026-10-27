<script setup lang="ts">
import { Music, Upload } from '@lucide/vue'
import { commands } from '~/ipc/bindings'
import type { Action, SoundMode } from '~/types/engine'

type PlaySound = Extract<Action, { type: 'play_sound' }>

const action = defineModel<PlaySound>({ required: true })

const { t } = useI18n()
const { upload } = useEngine()
const input = ref<HTMLInputElement | null>(null)
const outputs = ref<string[]>([])
const failed = ref(false)
const uploading = ref(false)

const DEFAULT_OUTPUT = '__default'
/** Each with its name and hint in `actions.sound.modes`. */
const MODES: SoundMode[] = ['restart', 'overlap', 'toggle', 'loop']
/** Some systems give audio files no type; the extension says what they are. */
const TYPES: Record<string, string> = { mp3: 'audio/mpeg', wav: 'audio/wav', m4a: 'audio/mp4', flac: 'audio/flac', ogg: 'audio/ogg' }

onMounted(async () => {
  outputs.value = await commands.audioOutputs()
})

const volume = computed({
  get: () => [action.value.volume],
  set: ([value]) => { action.value = { ...action.value, volume: value ?? 0 } },
})

async function choose(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) {
    return
  }
  failed.value = false
  uploading.value = true
  try {
    const type = file.type || TYPES[file.name.split('.').pop()?.toLowerCase() ?? ''] || ''
    const sound = await upload(file, type)
    action.value = { ...action.value, sound, name: file.name }
  }
  catch {
    failed.value = true
  }
  finally {
    uploading.value = false
    if (input.value) {
      input.value.value = ''
    }
  }
}

function setMode(value: unknown) {
  if (typeof value === 'string') {
    action.value = { ...action.value, mode: value as SoundMode }
  }
}

function setOutput(value: unknown) {
  if (typeof value === 'string') {
    action.value = { ...action.value, device: value === DEFAULT_OUTPUT ? null : value }
  }
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <div class="flex min-w-0 flex-1 items-center gap-2 rounded-md border px-3 py-1.5 text-sm">
        <Music class="size-4 shrink-0 text-muted-foreground" />
        <span class="truncate" :class="action.name ? '' : 'text-muted-foreground'">{{ action.name || t('actions.sound.none') }}</span>
      </div>
      <Button variant="outline" size="sm" :disabled="uploading" @click="input?.click()">
        <Upload /> {{ t(action.sound ? 'actions.sound.change' : 'actions.sound.choose') }}
      </Button>
      <input
        ref="input"
        type="file"
        accept="audio/*,.mp3,.wav,.m4a,.flac,.ogg"
        class="hidden"
        :aria-label="t('actions.sound.file')"
        @change="choose"
      >
    </div>
    <p v-if="failed" class="text-xs text-destructive">
      {{ t('actions.sound.uploadFailed') }}
    </p>

    <div class="flex items-center gap-3">
      <span class="w-16 text-xs text-muted-foreground">{{ t('actions.sound.volume') }}</span>
      <Slider v-model="volume" :max="100" class="flex-1" :aria-label="t('actions.sound.volume')" />
      <span class="w-10 text-right text-xs tabular-nums">{{ action.volume }} %</span>
    </div>

    <div class="flex flex-col gap-1">
      <Select :model-value="action.mode" @update:model-value="setMode">
        <SelectTrigger class="w-full" :aria-label="t('actions.sound.again')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="mode in MODES" :key="mode" :value="mode">
            {{ t(`actions.sound.modes.${mode}.name`) }}
          </SelectItem>
        </SelectContent>
      </Select>
      <p class="text-[11px] text-muted-foreground">
        {{ t(`actions.sound.modes.${action.mode}.hint`) }}
      </p>
    </div>

    <Select :model-value="action.device ?? DEFAULT_OUTPUT" @update:model-value="setOutput">
      <SelectTrigger class="w-full" :aria-label="t('actions.sound.output')">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem :value="DEFAULT_OUTPUT">
          {{ t('actions.sound.defaultOutput') }}
        </SelectItem>
        <SelectItem v-for="name in outputs" :key="name" :value="name">
          {{ name }}
        </SelectItem>
        <SelectItem v-if="action.device && !outputs.includes(action.device)" :value="action.device">
          {{ t('actions.sound.disconnected', { device: action.device }) }}
        </SelectItem>
      </SelectContent>
    </Select>
  </div>
</template>
