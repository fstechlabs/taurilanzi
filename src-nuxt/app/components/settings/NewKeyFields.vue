<script setup lang="ts">
import { refDebounced } from '@vueuse/core'
import type { VerticalPosition } from '~/types/engine'
import { fromHex, toHex } from '~/lib/color'
import { FONT_WEIGHTS } from '~/lib/fonts'
import type { NewKeyStyle } from '~/lib/new-key'
import { newKeyAppearance } from '~/lib/new-key'

const style = defineModel<NewKeyStyle>({ required: true })
const { previewUrl } = useEngine()
const { fonts } = useFonts()
const { t } = useI18n()

const background = computed({
  get: () => toHex(style.value.background),
  set: (hex) => { style.value.background = fromHex(hex) },
})
const textColor = computed({
  get: () => toHex(style.value.text_color),
  set: (hex) => { style.value.text_color = fromHex(hex) },
})
const weight = computed({
  get: () => String(style.value.font_weight),
  set: (value) => { style.value.font_weight = Number(value) },
})
const position = computed({
  get: () => style.value.text_position,
  set: (value) => {
    if (value) {
      style.value.text_position = value as VerticalPosition
    }
  },
})

const preview = refDebounced(computed(() => previewUrl(0, newKeyAppearance(style.value, t('editor.newKey.text')))), 120)
</script>

<template>
  <div class="flex gap-6 py-3">
    <div class="flex min-w-0 flex-1 flex-col divide-y divide-neutral-800">
      <SettingRow :label="t('settings.newKeys.background')" class="pt-0">
        <ColorPicker v-model="background" :label="t('settings.newKeys.backgroundLabel')" />
      </SettingRow>
      <SettingRow :label="t('settings.newKeys.textColor')">
        <ColorPicker v-model="textColor" :label="t('settings.newKeys.textColorLabel')" />
      </SettingRow>
      <SettingRow :label="t('settings.newKeys.font')">
        <Select v-model="style.font_family">
          <SelectTrigger class="w-40" :aria-label="t('settings.newKeys.fontLabel')">
            <SelectValue />
          </SelectTrigger>
          <SelectContent class="max-h-72">
            <SelectItem v-for="font in fonts" :key="font" :value="font">
              {{ font }}
            </SelectItem>
          </SelectContent>
        </Select>
        <Select v-model="weight">
          <SelectTrigger class="w-32" :aria-label="t('settings.newKeys.weightLabel')">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="w in FONT_WEIGHTS" :key="w.value" :value="String(w.value)">
              {{ w.label }}
            </SelectItem>
          </SelectContent>
        </Select>
      </SettingRow>
      <SettingRow :label="t('settings.newKeys.textSize')">
        <SliderField v-model="style.text_size" :label="t('settings.newKeys.textSizeLabel')" :min="10" :max="80" />
      </SettingRow>
      <SettingRow :label="t('settings.newKeys.position')" class="pb-0">
        <ToggleGroup v-model="position" type="single" variant="outline" size="sm" :aria-label="t('settings.newKeys.positionLabel')">
          <ToggleGroupItem value="top">
            {{ t('settings.newKeys.top') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="middle">
            {{ t('settings.newKeys.middle') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="bottom">
            {{ t('settings.newKeys.bottom') }}
          </ToggleGroupItem>
        </ToggleGroup>
      </SettingRow>
    </div>
    <div class="flex w-32 shrink-0 flex-col items-center gap-2 border-l border-neutral-800 pl-6">
      <img :src="preview" :alt="t('settings.newKeys.preview')" class="aspect-square w-24 rounded-xl bg-black">
      <span class="text-center text-[11px] text-muted-foreground">{{ t('settings.newKeys.preview') }}</span>
    </div>
  </div>
</template>
