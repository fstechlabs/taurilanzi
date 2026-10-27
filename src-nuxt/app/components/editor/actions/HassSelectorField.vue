<script setup lang="ts">
import { X } from '@lucide/vue'
import { fromHex, toHex } from '~/lib/color'

/** One field of an action of Home Assistant, edited as its selector says. Anything without an
 * editor of its own is written as JSON, or as text when it is not JSON. */
const value = defineModel<unknown>({ required: true })
const props = defineProps<{
  selector: Record<string, unknown>
  /** The action's domain, where its texts live. */
  domain: string
  label: string
}>()

const { t } = useI18n()
const hass = useHass()

const kind = computed(() => Object.keys(props.selector ?? {})[0] ?? 'object')
const config = computed(() => ((props.selector?.[kind.value] ?? {}) as Record<string, unknown>))

type Option = { value: string, label: string }
const selectOptions = computed<Option[]>(() => {
  const raw = config.value.options
  if (!Array.isArray(raw)) {
    return []
  }
  const key = config.value.translation_key as string | undefined
  return raw.map((option) => {
    if (option && typeof option === 'object') {
      const o = option as { value: unknown, label?: unknown }
      return { value: String(o.value), label: String(o.label ?? o.value) }
    }
    return { value: String(option), label: hass.selectorOption(props.domain, key, String(option)) }
  })
})

const multiple = computed(() => Boolean(config.value.multiple))

const numberValue = computed({
  get: () => (typeof value.value === 'number' ? value.value : value.value === undefined ? '' : Number(value.value)),
  set: (text: string | number) => {
    const n = Number(text)
    value.value = String(text).trim() === '' || !Number.isFinite(n) ? undefined : n
  },
})

const textValue = computed({
  get: () => (value.value === undefined || value.value === null ? '' : String(value.value)),
  set: (text: string) => { value.value = text === '' ? undefined : text },
})

const json = ref('')
const jsonError = ref(false)
watch(value, (next) => {
  if (!jsonError.value) {
    json.value = next === undefined ? '' : typeof next === 'string' ? next : JSON.stringify(next, null, 2)
  }
}, { immediate: true })

function setJson(text: string) {
  json.value = text
  if (!text.trim()) {
    jsonError.value = false
    value.value = undefined
    return
  }
  try {
    value.value = JSON.parse(text)
    jsonError.value = false
  }
  catch {
    // Plain text is a value too: a template, a message.
    value.value = text
    jsonError.value = false
  }
}

const color = computed({
  get: () => {
    const rgb = value.value
    return Array.isArray(rgb) && rgb.length >= 3 ? toHex({ r: Number(rgb[0]), g: Number(rgb[1]), b: Number(rgb[2]) }) : '#ffffff'
  },
  set: (hex: string) => {
    const { r, g, b } = fromHex(hex)
    value.value = [r, g, b]
  },
})

/** Several values, for selectors that take a list. */
const list = computed<string[]>(() => (Array.isArray(value.value) ? value.value.map(String) : value.value ? [String(value.value)] : []))

function add(item: string) {
  if (item && !list.value.includes(item)) {
    value.value = multiple.value ? [...list.value, item] : item
  }
}

function remove(item: string) {
  const rest = list.value.filter(i => i !== item)
  value.value = rest.length ? rest : undefined
}

const entityDomains = computed(() => {
  const filter = config.value.filter ?? config.value
  const filters = Array.isArray(filter) ? filter : [filter]
  const domains = filters.flatMap((f) => {
    const domain = (f as { domain?: unknown }).domain
    return Array.isArray(domain) ? domain.map(String) : domain ? [String(domain)] : []
  })
  return domains.length ? domains : null
})

const named = computed(() => {
  const catalog = hass.catalog.value
  switch (kind.value) {
    case 'area':
      return catalog?.areas ?? []
    case 'floor':
      return catalog?.floors ?? []
    case 'label':
      return catalog?.labels ?? []
    case 'device':
      return catalog?.devices ?? []
    default:
      return []
  }
})

const duration = computed(() => {
  const d = (value.value && typeof value.value === 'object' ? value.value : {}) as Record<string, number>
  return { hours: d.hours ?? 0, minutes: d.minutes ?? 0, seconds: d.seconds ?? 0 }
})

function setDuration(part: 'hours' | 'minutes' | 'seconds', amount: number) {
  value.value = { ...duration.value, [part]: Number.isFinite(amount) ? Math.max(0, Math.round(amount)) : 0 }
}

const dateTime = computed({
  get: () => (typeof value.value === 'string' ? value.value.replace(' ', 'T').slice(0, 16) : ''),
  set: (text: string) => { value.value = text ? `${text.replace('T', ' ')}:00` : undefined },
})
</script>

<template>
  <div class="flex flex-col gap-1">
    <Switch v-if="kind === 'boolean'" :model-value="Boolean(value)" :aria-label="props.label" @update:model-value="value = $event" />

    <div v-else-if="kind === 'number' || kind === 'color_temp'" class="flex items-center gap-2">
      <Input
        v-model="numberValue"
        type="number"
        :min="config.min as number | undefined"
        :max="config.max as number | undefined"
        :step="(config.step as number | string | undefined) ?? 'any'"
        class="h-8 w-32 text-right"
        :aria-label="props.label"
      />
      <span class="text-xs text-muted-foreground">{{ kind === 'color_temp' ? (config.unit === 'mired' ? 'mired' : 'K') : (config.unit_of_measurement as string ?? '') }}</span>
    </div>

    <template v-else-if="kind === 'select' && !multiple && selectOptions.length">
      <Select :model-value="textValue" @update:model-value="textValue = String($event)">
        <SelectTrigger class="w-full" :aria-label="props.label">
          <SelectValue :placeholder="t('actions.hass.call.choose')" />
        </SelectTrigger>
        <SelectContent class="max-h-72">
          <SelectItem v-for="o in selectOptions" :key="o.value" :value="o.value">
            {{ o.label }}
          </SelectItem>
        </SelectContent>
      </Select>
    </template>

    <ToggleGroup
      v-else-if="kind === 'select' && multiple && selectOptions.length"
      :model-value="list"
      type="multiple"
      variant="outline"
      size="sm"
      class="flex-wrap justify-start"
      :aria-label="props.label"
      @update:model-value="value = ($event as string[]).length ? $event : undefined"
    >
      <ToggleGroupItem v-for="o in selectOptions" :key="o.value" :value="o.value">
        {{ o.label }}
      </ToggleGroupItem>
    </ToggleGroup>

    <template v-else-if="kind === 'entity'">
      <div v-if="multiple && list.length" class="flex flex-wrap gap-1">
        <span v-for="item in list" :key="item" class="flex items-center gap-1 rounded bg-neutral-800 px-1.5 py-0.5 font-mono text-[11px]">
          {{ hass.entity(item)?.name ?? item }}
          <button type="button" :aria-label="t('actions.hass.call.remove', { name: item })" @click="remove(item)"><X class="size-3" /></button>
        </span>
      </div>
      <HassEntityField :model-value="multiple ? '' : textValue" :domains="entityDomains" :label="props.label" @update:model-value="multiple ? add($event) : (textValue = $event)" />
    </template>

    <template v-else-if="['area', 'floor', 'label', 'device'].includes(kind)">
      <div v-if="list.length" class="flex flex-wrap gap-1">
        <span v-for="item in list" :key="item" class="flex items-center gap-1 rounded bg-neutral-800 px-1.5 py-0.5 text-[11px]">
          {{ named.find(n => n.id === item)?.name ?? item }}
          <button type="button" :aria-label="t('actions.hass.call.remove', { name: item })" @click="remove(item)"><X class="size-3" /></button>
        </span>
      </div>
      <Select :model-value="''" @update:model-value="add(String($event))">
        <SelectTrigger class="w-full" :aria-label="props.label">
          <SelectValue :placeholder="t('actions.hass.call.addOne')" />
        </SelectTrigger>
        <SelectContent class="max-h-72">
          <SelectItem v-for="n in named" :key="n.id" :value="n.id">
            {{ n.name }}
          </SelectItem>
        </SelectContent>
      </Select>
    </template>

    <ColorPicker v-else-if="kind === 'color_rgb'" v-model="color" :label="props.label" />

    <Input v-else-if="kind === 'time'" v-model="textValue" type="time" step="1" class="h-8 w-40" :aria-label="props.label" />
    <Input v-else-if="kind === 'date'" v-model="textValue" type="date" class="h-8 w-44" :aria-label="props.label" />
    <Input v-else-if="kind === 'datetime'" v-model="dateTime" type="datetime-local" class="h-8 w-56" :aria-label="props.label" />

    <div v-else-if="kind === 'duration'" class="flex items-center gap-1.5 text-xs text-muted-foreground">
      <Input :model-value="duration.hours" type="number" min="0" class="h-8 w-16 text-right" :aria-label="t('actions.hass.call.hours')" @update:model-value="setDuration('hours', Number($event))" /> h
      <Input :model-value="duration.minutes" type="number" min="0" class="h-8 w-16 text-right" :aria-label="t('actions.hass.call.minutes')" @update:model-value="setDuration('minutes', Number($event))" /> min
      <Input :model-value="duration.seconds" type="number" min="0" class="h-8 w-16 text-right" :aria-label="t('actions.hass.call.seconds')" @update:model-value="setDuration('seconds', Number($event))" /> s
    </div>

    <Textarea v-else-if="kind === 'text' && config.multiline" v-model="textValue" rows="3" :aria-label="props.label" />
    <Input v-else-if="['text', 'icon', 'theme', 'conversation_agent', 'language', 'backup_location', 'assist_pipeline'].includes(kind)" v-model="textValue" :aria-label="props.label" />

    <template v-else>
      <Textarea :model-value="json" rows="3" class="font-mono text-xs" :aria-label="props.label" @update:model-value="setJson(String($event))" />
      <p class="text-[11px] text-muted-foreground">
        {{ t('actions.hass.call.jsonHint') }}
      </p>
    </template>
  </div>
</template>
