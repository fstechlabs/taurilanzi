<script setup lang="ts">
import { X } from '@lucide/vue'
import { HASS_SERVER } from '~/composables/useHass'
import type { HassTile } from '~/composables/useHassKey'
import { domainOf, drawingHasRange, suggestedExtra, suitedDrawing, tileDrawings } from '~/lib/actions/hass'
import type { TileDrawing } from '~/types/engine'

/** What a tile of Home Assistant draws and what it shows next to its entity, with the choices
 * that suit the kind of entity. */
const props = defineProps<{ widget: HassTile, server: string }>()
const emit = defineEmits<{ update: [changes: Partial<HassTile>] }>()

const { t } = useI18n()
const hass = useHass(() => props.server)
provide(HASS_SERVER, toRef(props, 'server'))

const info = computed(() => (props.widget.entity ? hass.entity(props.widget.entity) : undefined))
const choices = computed(() => tileDrawings(props.widget.entity, info.value))
const suited = computed(() => suitedDrawing(props.widget.entity, info.value))
const drawing = computed(() => props.widget.drawing ?? 'auto')
/** The drawing the tile shows now, the one the app picks when left to it. */
const shown = computed(() => (drawing.value === 'auto' ? suited.value : drawing.value))

function camel(text: string) {
  return text.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
}

function drawingName(value: TileDrawing) {
  return t(`widgets.hass.drawings.${camel(value)}`)
}

const autoName = computed(() => (suited.value === 'auto' ? drawingName('auto') : t('widgets.hass.autoDrawing', { drawing: drawingName(suited.value) })))

function setDrawing(value: unknown) {
  if (typeof value === 'string') {
    emit('update', { drawing: value as TileDrawing })
  }
}

const hasRange = computed(() => drawingHasRange(shown.value) && !['binary_sensor'].includes(domainOf(props.widget.entity)))

function setLimit(field: 'min' | 'max', value: string | number) {
  const text = String(value).trim().replace(',', '.')
  const number = Number(text)
  emit('update', { [field]: text === '' || !Number.isFinite(number) ? null : number })
}

function setHours(value: string | number) {
  const hours = Math.round(Number(value))
  if (Number.isFinite(hours)) {
    emit('update', { hours: Math.min(24 * 7, Math.max(1, hours)) })
  }
}

const suggestion = computed(() => {
  const found = suggestedExtra(props.widget.entity, hass.entities.value)
  return found && found !== props.widget.extra ? found : null
})
</script>

<template>
  <div class="flex flex-col gap-3" data-hass-tile>
    <div v-if="widget.entity" class="flex flex-col gap-1.5">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.drawing') }}</Label>
      <Select :model-value="drawing" @update:model-value="setDrawing">
        <SelectTrigger class="w-full" :aria-label="t('widgets.hass.drawing')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="auto">
            {{ autoName }}
          </SelectItem>
          <SelectItem v-for="d in choices" :key="d" :value="d">
            {{ drawingName(d) }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>

    <div v-if="hasRange" class="flex items-center gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.range') }}</Label>
      <Input :model-value="widget.min ?? ''" type="number" class="h-8 w-24 text-right" :placeholder="t('widgets.hass.rangeAuto')" :aria-label="t('widgets.hass.min')" @update:model-value="setLimit('min', $event)" />
      <span class="text-xs text-muted-foreground">–</span>
      <Input :model-value="widget.max ?? ''" type="number" class="h-8 w-24 text-right" :placeholder="t('widgets.hass.rangeAuto')" :aria-label="t('widgets.hass.max')" @update:model-value="setLimit('max', $event)" />
    </div>

    <div v-if="shown === 'graph'" class="flex items-center gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.period') }}</Label>
      <Input :model-value="widget.hours ?? 24" type="number" min="1" max="168" class="h-8 w-20 text-right" :aria-label="t('widgets.hass.hours')" @update:model-value="setHours" />
      <span class="text-xs text-muted-foreground">{{ t('widgets.hass.hoursUnit') }}</span>
    </div>

    <div class="flex flex-col gap-1.5">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.extra') }}</Label>
      <div class="flex items-center gap-1.5">
        <div class="min-w-0 flex-1">
          <HassEntityField :model-value="widget.extra ?? ''" :domains="['sensor', 'binary_sensor', 'number', 'input_number', 'counter', 'climate', 'weather']" :label="t('widgets.hass.extra')" @update:model-value="emit('update', { extra: $event || null })" />
        </div>
        <Button v-if="widget.extra" variant="ghost" size="icon-sm" :aria-label="t('widgets.hass.extraRemove')" :title="t('widgets.hass.extraRemove')" @click="emit('update', { extra: null })">
          <X />
        </Button>
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.hass.extraHint') }}
        <button v-if="suggestion" type="button" class="underline hover:text-foreground" @click="emit('update', { extra: suggestion })">
          {{ t('widgets.hass.extraSuggest', { name: hass.entity(suggestion)?.name ?? suggestion }) }}
        </button>
      </p>
    </div>
  </div>
</template>
