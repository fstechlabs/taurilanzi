<script setup lang="ts">
import type { StateDraft } from '~/composables/useKeyEditor'

const draft = defineModel<StateDraft>({ required: true })
const { t } = useI18n()
const angle = computed({
  get: () => [draft.value.angle],
  set: ([value]) => { draft.value.angle = value ?? 0 },
})
</script>

<template>
  <div class="flex flex-col gap-2">
    <Label class="text-xs text-muted-foreground">{{ t('editor.background.title') }}</Label>
    <ToggleGroup :model-value="draft.background" type="single" @update:model-value="v => v && (draft.background = v as StateDraft['background'])" variant="outline" size="sm" class="w-full">
      <ToggleGroupItem value="solid" class="flex-1">
        {{ t('editor.background.solid') }}
      </ToggleGroupItem>
      <ToggleGroupItem value="gradient" class="flex-1">
        {{ t('editor.background.gradient') }}
      </ToggleGroupItem>
    </ToggleGroup>
    <div class="flex gap-2">
      <ColorPicker v-model="draft.from" :label="draft.background === 'gradient' ? t('editor.background.from') : t('editor.background.color')" />
      <ColorPicker v-if="draft.background === 'gradient'" v-model="draft.to" :label="t('editor.background.to')" />
    </div>
    <div v-if="draft.background === 'gradient'" class="flex items-center gap-3">
      <span class="w-14 text-xs text-muted-foreground">{{ t('editor.background.angle') }}</span>
      <Slider v-model="angle" :max="360" :step="5" class="flex-1" />
      <span class="w-10 text-right text-xs tabular-nums">{{ draft.angle }}°</span>
    </div>
  </div>
</template>
