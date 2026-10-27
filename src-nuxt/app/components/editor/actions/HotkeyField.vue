<script setup lang="ts">
import { Circle, Square } from '@lucide/vue'
import type { Modifier } from '~/lib/actions/keys'
import { KEY_CODES, MODIFIER_ORDER, comboLabel, isModifier, keyLabel, modifierName, modifierSymbol } from '~/lib/actions/keys'
import { os } from '~/lib/os'
import type { KeyCombo } from '~/types/engine'

const keys = defineModel<KeyCombo>({ required: true })

const { t } = useI18n()
const recording = ref(false)


/** Caught before anything else on the page, so Delete does not remove a line meanwhile. */
function capture(event: KeyboardEvent) {
  event.preventDefault()
  event.stopPropagation()
  if (isModifier(event.code)) {
    return
  }
  keys.value = {
    key: event.code,
    command: event.metaKey,
    shift: event.shiftKey,
    option: event.altKey,
    control: event.ctrlKey,
  }
  recording.value = false
}

watch(recording, (on) => {
  if (on) {
    window.addEventListener('keydown', capture, true)
  }
  else {
    window.removeEventListener('keydown', capture, true)
  }
})

onBeforeUnmount(() => window.removeEventListener('keydown', capture, true))

function toggle(flag: Modifier) {
  keys.value = { ...keys.value, [flag]: !keys.value[flag] }
}

function setKey(value: unknown) {
  if (typeof value === 'string') {
    keys.value = { ...keys.value, key: value }
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2">
      <div
        class="flex h-9 min-w-0 flex-1 items-center rounded-md border px-3 text-sm"
        :class="recording ? 'border-sky-500 ring-2 ring-sky-500/30' : ''"
        aria-live="polite"
        data-shortcut
      >
        <span v-if="recording" class="text-sky-400">{{ t('actions.hotkey.recording') }}</span>
        <span v-else-if="keys.key" class="font-mono tracking-wide">{{ comboLabel(keys) }}</span>
        <span v-else class="text-muted-foreground">{{ t('actions.hotkey.none') }}</span>
      </div>
      <Button :variant="recording ? 'secondary' : 'outline'" size="sm" @click="recording = !recording">
        <component :is="recording ? Square : Circle" :class="recording ? '' : 'text-red-400'" />
        {{ t(recording ? 'actions.hotkey.cancel' : 'actions.hotkey.record') }}
      </Button>
    </div>
    <div class="flex flex-wrap items-center gap-1.5">
      <Toggle
        v-for="flag in MODIFIER_ORDER"
        :key="flag"
        :model-value="keys[flag]"
        variant="outline"
        size="sm"
        class="min-w-9 font-mono"
        :aria-label="modifierName(flag)"
        :title="modifierName(flag)"
        @update:model-value="toggle(flag)"
      >
        {{ modifierSymbol(flag) }}
      </Toggle>
      <Select :model-value="keys.key" @update:model-value="setKey">
        <SelectTrigger size="sm" class="min-w-28 flex-1" :aria-label="t('actions.hotkey.key')">
          <SelectValue :placeholder="t('actions.hotkey.keyPlaceholder')" />
        </SelectTrigger>
        <SelectContent class="max-h-72">
          <SelectItem v-for="code in KEY_CODES" :key="code" :value="code">
            {{ keyLabel(code) }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>
    <p class="text-[11px] text-muted-foreground">
      {{ t(`actions.hotkey.hint.${os}`) }}
    </p>
  </div>
</template>
