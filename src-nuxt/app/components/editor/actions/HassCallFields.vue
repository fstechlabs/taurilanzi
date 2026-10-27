<script setup lang="ts">
import { HASS_SERVER } from '~/composables/useHass'
import { Check, ChevronsUpDown, X } from '@lucide/vue'
import { normalize } from '~/lib/actions/catalog'
import { domainOf, hassStatusText } from '~/lib/actions/hass'
import type { HassCommand, HassTarget } from '~/types/engine'

type Call = Extract<HassCommand, { kind: 'call' }>
interface FieldSchema {
  selector?: Record<string, unknown>
  required?: boolean
  example?: unknown
  default?: unknown
  filter?: { supported_features?: number[], attribute?: Record<string, unknown[]> }
  /** A section of fields, such as advanced options. */
  fields?: Record<string, FieldSchema>
  collapsed?: boolean
}
interface ServiceSchema {
  fields?: Record<string, FieldSchema>
  target?: { entity?: { domain?: string[] }[] }
}
interface Field { name: string, schema: FieldSchema, section: string | null }

/** Any action of any integration of Home Assistant, with a form made from its fields. */
const command = defineModel<Call>({ required: true })
const props = defineProps<{ server: string }>()

const { t } = useI18n()
const hass = useHass(() => props.server)
provide(HASS_SERVER, toRef(props, 'server'))
const term = ref('')
const raw = ref(false)

onMounted(() => hass.connect())

const services = computed(() => (hass.catalog.value?.services ?? {}) as Record<string, Record<string, ServiceSchema>>)

const actions = computed(() => Object.entries(services.value).flatMap(([domain, list]) =>
  Object.keys(list).map((service) => {
    const action = `${domain}.${service}`
    return { action, domain: hass.domainName(domain), name: hass.serviceName(action) }
  })).sort((a, b) => a.domain.localeCompare(b.domain) || a.name.localeCompare(b.name)))

const matches = computed(() => {
  const words = normalize(term.value).split(/\s+/).filter(Boolean)
  return actions.value
    .filter(a => words.every(word => normalize(`${a.domain} ${a.name} ${a.action}`).includes(word)))
    .slice(0, 150)
})

const schema = computed<ServiceSchema | null>(() => {
  const [domain, service] = command.value.action.split('.')
  return (domain && service && services.value[domain]?.[service]) || null
})
const current = computed(() => actions.value.find(a => a.action === command.value.action))

function setAction(action: string) {
  if (!action || action === command.value.action) {
    return
  }
  const [domain] = action.split('.')
  const service = action.split('.')[1] ?? ''
  const next = services.value[domain ?? '']?.[service]
  command.value = {
    ...command.value,
    action,
    target: next?.target ? command.value.target : emptyTarget(),
    data: {},
  }
}

function emptyTarget(): Required<HassTarget> {
  return { entities: [], devices: [], areas: [], floors: [], labels: [] }
}

const target = computed(() => ({ ...emptyTarget(), ...command.value.target }) as Required<HassTarget>)
const targetDomains = computed(() => {
  const domains = (schema.value?.target?.entity ?? []).flatMap(e => e.domain ?? [])
  return domains.length ? domains : null
})

function setTarget(part: keyof HassTarget, ids: string[]) {
  command.value = { ...command.value, target: { ...target.value, [part]: ids } }
}

function addTo(part: keyof HassTarget, id: string) {
  if (id && !target.value[part].includes(id)) {
    setTarget(part, [...target.value[part], id])
  }
}

const groups = computed(() => {
  const catalog = hass.catalog.value
  return ([
    ['areas', catalog?.areas ?? []],
    ['devices', catalog?.devices ?? []],
    ['floors', catalog?.floors ?? []],
    ['labels', catalog?.labels ?? []],
  ] as const).filter(([, list]) => list.length)
})

/** Features of the entities targeted, to hide the fields none of them supports. */
const targeted = computed(() => target.value.entities.map(id => hass.entity(id)).filter(Boolean))

function supported(field: FieldSchema) {
  const features = field.filter?.supported_features
  if (!features?.length || !targeted.value.length) {
    return true
  }
  return targeted.value.some(e => features.some(f => (Number(e!.attributes.supported_features ?? 0) & f) === f))
}

const fields = computed<Field[]>(() => Object.entries(schema.value?.fields ?? {}).flatMap(([name, field]): Field[] => {
  if (field.fields) {
    return Object.entries(field.fields).map(([inner, schema]) => ({ name: inner, schema, section: name }))
  }
  return [{ name, schema: field, section: null }]
}).filter(f => f.schema.selector && supported(f.schema)))

const data = computed(() => (command.value.data ?? {}) as Record<string, unknown>)

function setField(name: string, value: unknown) {
  const next = { ...data.value }
  if (value === undefined) {
    delete next[name]
  }
  else {
    next[name] = value
  }
  command.value = { ...command.value, data: next }
}

const json = ref('')
const jsonError = ref(false)
watch(raw, (on) => {
  if (on) {
    json.value = JSON.stringify(data.value, null, 2)
    jsonError.value = false
  }
})

function setJson(text: string) {
  json.value = text
  try {
    const parsed = text.trim() ? JSON.parse(text) as unknown : {}
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error('not an object')
    }
    jsonError.value = false
    command.value = { ...command.value, data: parsed as Record<string, unknown> }
  }
  catch {
    jsonError.value = true
  }
}

function example(field: FieldSchema) {
  return field.example === undefined ? '' : t('actions.hass.call.example', { example: typeof field.example === 'string' ? field.example : JSON.stringify(field.example) })
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <p v-if="!hass.connected.value && hass.info.value" class="rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-200">
      {{ t('actions.hass.notConnected', { status: hassStatusText(hass.info.value) }) }}
    </p>

    <div class="flex flex-col gap-1.5">
      <Label class="text-xs text-muted-foreground">{{ t('actions.hass.call.action') }}</Label>
      <Combobox :model-value="command.action" ignore-filter @update:model-value="setAction(String($event))" @update:open="open => !open && (term = '')">
        <ComboboxAnchor as-child>
          <ComboboxTrigger as-child>
            <Button variant="outline" class="h-auto min-h-9 w-full justify-between py-1.5 font-normal" :aria-label="t('actions.hass.call.action')" data-hass-action>
              <span v-if="command.action" class="min-w-0 truncate text-left">
                {{ current ? `${current.domain}: ${current.name}` : command.action }}
                <span class="block truncate font-mono text-[11px] text-muted-foreground">{{ command.action }}</span>
              </span>
              <span v-else class="text-muted-foreground">{{ t('actions.hass.call.chooseAction') }}</span>
              <ChevronsUpDown class="opacity-50" />
            </Button>
          </ComboboxTrigger>
        </ComboboxAnchor>
        <ComboboxList class="w-[32rem]" align="start">
          <ComboboxInput v-model="term" :placeholder="t('actions.hass.call.search')" :aria-label="t('actions.hass.call.search')" />
          <ComboboxViewport class="max-h-80">
            <ComboboxItem v-for="a in matches" :key="a.action" :value="a.action" :text-value="a.name" :data-action="a.action">
              <span class="min-w-0 flex-1">
                <span class="block truncate">{{ a.domain }}: {{ a.name }}</span>
                <span class="block truncate font-mono text-[11px] text-muted-foreground">{{ a.action }}</span>
              </span>
              <ComboboxItemIndicator><Check /></ComboboxItemIndicator>
            </ComboboxItem>
            <p v-if="!matches.length" class="px-3 py-6 text-center text-sm text-muted-foreground">
              {{ hass.connected.value ? t('actions.hass.call.noActions') : t('actions.hass.call.typeAction') }}
            </p>
          </ComboboxViewport>
        </ComboboxList>
      </Combobox>
      <Input
        v-if="!hass.connected.value"
        :model-value="command.action"
        placeholder="light.turn_on"
        class="font-mono text-xs"
        :aria-label="t('actions.hass.call.actionId')"
        @update:model-value="command = { ...command, action: String($event).trim() }"
      />
      <p v-if="command.action && hass.serviceDescription(command.action)" class="text-[11px] text-muted-foreground">
        {{ hass.serviceDescription(command.action) }}
      </p>
    </div>

    <div v-if="schema?.target || !hass.connected.value" class="flex flex-col gap-2" data-hass-target>
      <Label class="text-xs text-muted-foreground">{{ t('actions.hass.call.target') }}</Label>
      <div v-if="target.entities.length" class="flex flex-wrap gap-1">
        <span v-for="id in target.entities" :key="id" class="flex items-center gap-1 rounded bg-neutral-800 px-1.5 py-0.5 text-[11px]">
          {{ hass.entity(id)?.name ?? id }}
          <button type="button" :aria-label="t('actions.hass.call.remove', { name: id })" @click="setTarget('entities', target.entities.filter(e => e !== id))"><X class="size-3" /></button>
        </span>
      </div>
      <HassEntityField :model-value="''" :domains="targetDomains" :label="t('actions.hass.call.addEntity')" @update:model-value="addTo('entities', $event)" />
      <div v-for="[part, list] in groups" :key="part" class="flex flex-col gap-1">
        <div v-if="target[part].length" class="flex flex-wrap gap-1">
          <span v-for="id in target[part]" :key="id" class="flex items-center gap-1 rounded bg-neutral-800 px-1.5 py-0.5 text-[11px]">
            {{ list.find(n => n.id === id)?.name ?? id }}
            <button type="button" :aria-label="t('actions.hass.call.remove', { name: id })" @click="setTarget(part, target[part].filter(e => e !== id))"><X class="size-3" /></button>
          </span>
        </div>
        <Select :model-value="''" @update:model-value="addTo(part, String($event))">
          <SelectTrigger class="w-full" :aria-label="t(`actions.hass.call.addTo.${part}`)">
            <SelectValue :placeholder="t(`actions.hass.call.addTo.${part}`)" />
          </SelectTrigger>
          <SelectContent class="max-h-72">
            <SelectItem v-for="n in list" :key="n.id" :value="n.id">
              {{ n.name }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
    </div>

    <div v-if="command.action" class="flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <Label class="text-xs text-muted-foreground">{{ t('actions.hass.call.fields') }}</Label>
        <label class="flex items-center gap-2 text-xs text-muted-foreground">
          {{ t('actions.hass.call.raw') }}
          <Switch v-model="raw" :aria-label="t('actions.hass.call.raw')" />
        </label>
      </div>
      <template v-if="raw || !fields.length">
        <Textarea :model-value="raw ? json : JSON.stringify(data, null, 2)" rows="6" class="font-mono text-xs" :aria-label="t('actions.hass.call.data')" :aria-invalid="jsonError" @update:model-value="setJson(String($event))" @focus="raw = true" />
        <p class="text-[11px]" :class="jsonError ? 'text-destructive' : 'text-muted-foreground'">
          {{ jsonError ? t('actions.hass.jsonError') : fields.length ? t('actions.hass.call.rawHint') : t('actions.hass.call.noFields') }}
        </p>
      </template>
      <template v-else>
        <div v-for="field in fields" :key="`${field.section}.${field.name}`" class="flex flex-col gap-1 rounded-md border p-2" :data-field="field.name">
          <div class="flex items-start gap-2">
            <div class="min-w-0 flex-1">
              <p class="text-xs font-medium">
                {{ hass.fieldName(command.action, field.name) }}
                <span v-if="field.schema.required" class="text-destructive">*</span>
              </p>
              <p class="text-[11px] text-muted-foreground">
                {{ hass.fieldDescription(command.action, field.name) }} {{ example(field.schema) }}
              </p>
            </div>
            <Button v-if="data[field.name] !== undefined" variant="ghost" size="icon-sm" :aria-label="t('actions.hass.call.clear', { name: field.name })" @click="setField(field.name, undefined)">
              <X />
            </Button>
          </div>
          <HassSelectorField
            :model-value="data[field.name]"
            :selector="field.schema.selector!"
            :domain="domainOf(command.action)"
            :label="hass.fieldName(command.action, field.name)"
            @update:model-value="setField(field.name, $event)"
          />
        </div>
      </template>
    </div>
  </div>
</template>
