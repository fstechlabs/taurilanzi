<script setup lang="ts">
import { HASS_SERVER } from '~/composables/useHass'
import { Plus, RotateCcw, Search, X } from '@lucide/vue'
import { hassStatusText } from '~/lib/actions/hass'
import { preload, thumbnail } from '~/lib/iconify'
import type { PickedIcon } from '~/composables/useIconCatalog'
import { HASS_ENTITIES_MAX } from '~/lib/widgets'
import type { Widget } from '~/types/engine'
import { NO_SERVER } from '~/lib/integrations'

/** The fields of a widget of Home Assistant, edited in place like the rest of the draft. */
const props = defineProps<{ widget: Widget }>()

const { t } = useI18n()
const server = computed({
  get: () => ('server' in props.widget ? props.widget.server ?? NO_SERVER : NO_SERVER),
  set: (id: string) => {
    if ('server' in props.widget) {
      props.widget.server = id
    }
  },
})
const hass = useHass(server)
provide(HASS_SERVER, server)
const { assetUrl } = useEngine()

onMounted(() => hass.connect())

type Of<K extends Widget['kind']> = Extract<Widget, { kind: K }>
function only<K extends Widget['kind']>(kind: K) {
  return computed(() => (props.widget.kind === kind ? props.widget as Of<K> : null))
}
const entity = only('hass_entity')
const graph = only('hass_graph')
const several = only('hass_entities')
const picture = only('hass_picture')
const weather = only('hass_weather')
const template = only('hass_template')

/** Attributes worth showing on their own: not the ones that only describe the entity. */
const HIDDEN = new Set(['friendly_name', 'icon', 'entity_picture', 'supported_features', 'supported_color_modes', 'attribution', 'access_token', 'editable', 'id', 'user_id', 'device_trackers'])
const STATE = '-'

const attributes = computed(() => {
  const found = entity.value ? hass.entity(entity.value.entity) : undefined
  return Object.entries(found?.attributes ?? {})
    .filter(([name, value]) => !HIDDEN.has(name) && (typeof value !== 'object' || value === null || Array.isArray(value)))
    .map(([name]) => name)
})

const attribute = computed({
  get: () => entity.value?.attribute ?? STATE,
  set: (value: string) => {
    if (entity.value) {
      entity.value.attribute = value === STATE ? null : value
    }
  },
})

const keyIcons = useHassKey(null, server)
const picking = ref(false)
const iconFailed = ref(false)

/** Home Assistant's icons for the entity, so the card follows its state; refreshed with it. */
async function refreshIcons(id: string) {
  const widget = entity.value
  if (!widget || !id || !hass.entity(id)) {
    return
  }
  try {
    widget.icons = await keyIcons.icons(id)
    iconFailed.value = false
  }
  catch {
    iconFailed.value = true
  }
}

watch(() => [entity.value?.entity, hass.catalog.value] as const, ([id], before) => {
  const widget = entity.value
  if (id && widget && (id !== before?.[0] || !Object.keys(widget.icons ?? {}).length)) {
    refreshIcons(id)
  }
}, { immediate: true })

async function pickIcon({ icon }: PickedIcon) {
  if (entity.value) {
    entity.value.icon = await keyIcons.uploadIcon(icon)
  }
}

const iconPreview = computed(() => {
  const widget = entity.value
  if (widget?.icon) {
    return { src: assetUrl(widget.icon), own: true }
  }
  const name = widget?.entity ? hass.entityIcon(widget.entity) : null
  void iconsLoaded.value
  return { src: name ? thumbnail(name) : null, own: false }
})
const iconsLoaded = ref(0)
watch(() => (entity.value?.entity ? hass.entityIcon(entity.value.entity) : null), async (name) => {
  if (name) {
    await preload([name])
    iconsLoaded.value++
  }
}, { immediate: true })

function setHours(value: string | number) {
  const hours = Math.round(Number(value))
  if (graph.value && Number.isFinite(hours)) {
    graph.value.hours = Math.min(24 * 7, Math.max(1, hours))
  }
}

function setSeconds(value: string | number) {
  const seconds = Math.round(Number(value))
  if (picture.value && Number.isFinite(seconds)) {
    picture.value.seconds = Math.min(3600, Math.max(2, seconds))
  }
}

function addEntity() {
  if (several.value && several.value.entities.length < HASS_ENTITIES_MAX) {
    several.value.entities.push({ entity: '', label: '' })
  }
}

const PICTURES = ['camera', 'image', 'media_player', 'person', 'update']
const TEMPLATE_EXAMPLE = "{{ states('sensor.temperature') }} °C"
const NUMBERS = ['sensor', 'number', 'input_number', 'counter', 'climate', 'water_heater', 'humidifier']

const entityLabel = computed(() => {
  const id = entity.value?.entity ?? graph.value?.entity
  return (id && hass.entity(id)?.name) || t('widgets.hass.labelPlaceholder')
})
</script>

<template>
  <div class="flex flex-col gap-4">
    <ServerField v-model="server" kind="home_assistant" />
    <template v-if="entity">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.entity') }}</Label>
        <HassEntityField v-model="entity.entity" />
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.shows') }}</Label>
        <Select v-model="attribute">
          <SelectTrigger class="w-full" :aria-label="t('widgets.hass.shows')">
            <SelectValue />
          </SelectTrigger>
          <SelectContent class="max-h-72">
            <SelectItem :value="STATE">
              {{ t('widgets.hass.state') }}
            </SelectItem>
            <SelectItem v-for="a in attributes" :key="a" :value="a">
              {{ hass.attributeName(entity.entity, a) }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div class="flex items-center gap-3">
        <div class="flex size-12 shrink-0 items-center justify-center rounded-lg bg-neutral-800 p-2">
          <img v-if="iconPreview.src" :src="iconPreview.src" alt="" class="size-full" :class="iconPreview.own ? 'invert' : ''">
        </div>
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{{ t('actions.hass.look.icon') }}</span>
          <span class="truncate text-sm">{{ iconFailed ? t('widgets.hass.iconFailed') : iconPreview.own ? t('actions.hass.look.ownIcon') : t('actions.hass.look.haIconLive') }}</span>
        </div>
        <Button variant="outline" size="sm" :disabled="!entity.entity" @click="picking = true">
          <Search /> {{ t('actions.hass.look.changeIcon') }}
        </Button>
        <Button v-if="iconPreview.own" variant="ghost" size="icon-sm" :aria-label="t('actions.hass.look.resetIcon')" :title="t('actions.hass.look.resetIcon')" @click="entity.icon = null">
          <RotateCcw />
        </Button>
      </div>
      <IconPicker v-model:open="picking" @pick="pickIcon" />
    </template>

    <template v-if="graph">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.entity') }}</Label>
        <HassEntityField v-model="graph.entity" :domains="NUMBERS" />
      </div>
      <div class="flex items-center gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.period') }}</Label>
        <Input :model-value="graph.hours ?? 24" type="number" min="1" max="168" class="h-8 w-20 text-right" :aria-label="t('widgets.hass.hours')" @update:model-value="setHours" />
        <span class="text-xs text-muted-foreground">{{ t('widgets.hass.hoursUnit') }}</span>
      </div>
    </template>

    <div v-if="entity || graph" class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
      <Input v-if="entity" v-model="entity.label" :placeholder="entityLabel" maxlength="32" :aria-label="t('widgets.hass.label')" />
      <Input v-else-if="graph" v-model="graph.label" :placeholder="entityLabel" maxlength="32" :aria-label="t('widgets.hass.label')" />
    </div>

    <div v-if="several" class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.entities') }}</Label>
      <div v-for="(item, i) in several.entities" :key="i" class="flex flex-col gap-1.5 rounded-md bg-neutral-900/60 p-2" data-hass-item>
        <div class="flex items-center gap-1.5">
          <div class="min-w-0 flex-1">
            <HassEntityField v-model="item.entity" />
          </div>
          <Button variant="ghost" size="icon" class="size-8 shrink-0" :aria-label="t('widgets.hass.removeEntity')" :disabled="several.entities.length < 2" @click="several.entities.splice(i, 1)">
            <X />
          </Button>
        </div>
        <Input v-model="item.label" :placeholder="hass.entity(item.entity)?.name ?? t('widgets.hass.labelPlaceholder')" maxlength="16" :aria-label="t('widgets.hass.itemLabel')" />
      </div>
      <Button v-if="several.entities.length < HASS_ENTITIES_MAX" variant="outline" size="sm" @click="addEntity">
        <Plus /> {{ t('widgets.hass.addEntity') }}
      </Button>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.hass.entitiesHint', { max: HASS_ENTITIES_MAX }) }}
      </p>
    </div>

    <template v-if="picture">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.entity') }}</Label>
        <HassEntityField v-model="picture.entity" :domains="PICTURES" />
      </div>
      <div v-if="picture.entity.startsWith('camera.')" class="flex items-center gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.every') }}</Label>
        <Input :model-value="picture.seconds ?? 10" type="number" min="2" max="3600" class="h-8 w-20 text-right" :aria-label="t('widgets.hass.seconds')" @update:model-value="setSeconds" />
        <span class="text-xs text-muted-foreground">{{ t('widgets.fields.secondsUnit') }}</span>
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.hass.pictureHint') }}
      </p>
    </template>

    <template v-if="weather">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.entity') }}</Label>
        <HassEntityField v-model="weather.entity" :domains="['weather']" />
      </div>
      <label class="flex items-center justify-between gap-2 text-sm">
        <span>{{ t('widgets.hass.forecast') }} <span class="block text-[11px] text-muted-foreground">{{ t('widgets.hass.forecastHint') }}</span></span>
        <Switch v-model="weather.forecast" :aria-label="t('widgets.hass.forecast')" />
      </label>
    </template>

    <template v-if="template">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.hass.template') }}</Label>
        <Textarea v-model="template.template" rows="4" class="font-mono text-xs" :placeholder="TEMPLATE_EXAMPLE" :aria-label="t('widgets.hass.template')" />
        <p class="text-[11px] text-muted-foreground">
          {{ t('widgets.hass.templateHint') }}
        </p>
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
        <Input v-model="template.label" maxlength="32" :aria-label="t('widgets.hass.label')" />
      </div>
    </template>

    <p v-if="!hass.connected.value && hass.info.value" class="text-[11px] text-muted-foreground">
      {{ t('actions.hass.notConnected', { status: hassStatusText(hass.info.value) }) }}
      <NuxtLink to="/settings?tab=integrations" class="underline hover:text-foreground">
        {{ t('actions.hass.settingsLink') }}
      </NuxtLink>
    </p>
  </div>
</template>
