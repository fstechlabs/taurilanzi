<script setup lang="ts">
import { HASS_SERVER } from '~/composables/useHass'
import { KEY_DRAFT } from '~/lib/actions/key-draft'
import {
  DOMAINS_OF,
  canSwitch,
  choicesOf,
  domainOf,
  hassStatusText,
  levelUnit,
  levelsOf,
  optionAttribute,
  optionsOf,
  switchLabels,
} from '~/lib/actions/hass'
import { fromHex, toHex } from '~/lib/color'
import type { HassAlarm, HassCommand, HassLevel, HassMedia, HassOption } from '~/types/engine'

const command = defineModel<HassCommand>({ required: true })
const props = defineProps<{ server: string }>()

const { t } = useI18n()
const hass = useHass(() => props.server)
provide(HASS_SERVER, toRef(props, 'server'))
const look = useHassKey(inject(KEY_DRAFT, null), () => props.server)

const MEDIA: HassMedia[] = ['play_pause', 'play', 'pause', 'stop', 'next', 'previous', 'volume_up', 'volume_down', 'mute', 'shuffle']
const ALARM: HassAlarm[] = ['disarm', 'arm_home', 'arm_away', 'arm_night', 'arm_vacation', 'arm_custom_bypass', 'trigger']
const DEFAULT_STEPS: Record<HassLevel, number> = {
  brightness: 10,
  color_temp: 500,
  position: 10,
  tilt: 10,
  speed: 10,
  volume: 5,
  temperature: 0.5,
  humidity: 5,
  value: 1,
}

onMounted(() => hass.connect())

function camel(text: string) {
  return text.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
}

/** Edits some fields of the command, whatever its kind, keeping the rest. */
function patch(changes: Record<string, unknown>) {
  command.value = { ...command.value, ...changes } as HassCommand
}

const entityId = computed(() => ('entity' in command.value ? command.value.entity : ''))
const info = computed(() => (entityId.value ? hass.entity(entityId.value) : undefined))
const domains = computed(() => DOMAINS_OF[command.value.kind])
const switchable = computed(() => command.value.kind !== 'switch' || !entityId.value || canSwitch(entityId.value))

/** A new entity keeps the command valid for its domain, and gives a new key its icon and name. */
function setEntity(entity: string) {
  const value = command.value
  if (value.kind === 'level') {
    const levels = levelsOf(entity, hass.entity(entity))
    const level = levels.includes(value.level) ? value.level : (levels[0] ?? value.level)
    const step = DEFAULT_STEPS[level]
    patch({ entity, level, change: level === value.level ? value.change : { type: 'step', by: step } })
  }
  else if (value.kind === 'choose') {
    const options = optionsOf(entity)
    const option = options.includes(value.option) ? value.option : (options[0] ?? value.option)
    patch({ entity, option, choice: options.includes(value.option) ? value.choice : { type: 'next' } })
  }
  else {
    patch({ entity })
  }
  look.dress(entity)
}

const labels = computed(() => (command.value.kind === 'switch' && entityId.value ? switchLabels(entityId.value) : null))

function setMode(value: unknown) {
  if (typeof value === 'string' && value) {
    patch({ mode: value })
  }
}

const level = computed(() => (command.value.kind === 'level' ? command.value : null))
const levels = computed(() => (entityId.value ? levelsOf(entityId.value, info.value) : []))
const unit = computed(() => (level.value ? levelUnit(level.value.level, info.value) : ''))
/** A level the entity turns out not to support, which Home Assistant would refuse, moves to one it does. */
watch(levels, (now) => {
  const value = level.value
  if (value && now.length && !now.includes(value.level)) {
    patch({ level: now[0], change: { type: 'step', by: DEFAULT_STEPS[now[0]!] } })
  }
}, { immediate: true })
type Way = 'up' | 'down' | 'set'
const way = computed<Way>(() => {
  const change = level.value?.change
  if (!change || change.type === 'set') {
    return 'set'
  }
  return change.by < 0 ? 'down' : 'up'
})
const amount = computed(() => {
  const change = level.value?.change
  if (!change) {
    return 0
  }
  return change.type === 'set' ? change.to : Math.abs(change.by)
})

function setLevel(value: unknown) {
  if (typeof value === 'string' && level.value) {
    const step = DEFAULT_STEPS[value as HassLevel]
    patch({ level: value, change: { type: 'step', by: way.value === 'down' ? -step : step } })
  }
}

function setChange(next: Way, value: number) {
  const number = Number.isFinite(value) ? value : 0
  patch({ change: next === 'set' ? { type: 'set', to: number } : { type: 'step', by: next === 'down' ? -Math.abs(number) : Math.abs(number) } })
}

function setWay(value: unknown) {
  if (value !== 'up' && value !== 'down' && value !== 'set') {
    return
  }
  const current = level.value
  if (!current) {
    return
  }
  const step = DEFAULT_STEPS[current.level]
  const start = value === 'set'
    ? (way.value === 'set' ? amount.value : (current.level === 'color_temp' ? 3000 : current.level === 'temperature' ? 21 : 50))
    : (way.value === 'set' ? step : amount.value)
  setChange(value, start)
}

const choose = computed(() => (command.value.kind === 'choose' ? command.value : null))
const options = computed(() => (entityId.value ? optionsOf(entityId.value) : []))
const choices = computed(() => (choose.value ? choicesOf(info.value, choose.value.option) : []))

function choiceText(value: string) {
  return choose.value ? hass.stateText(entityId.value, value, optionAttribute(entityId.value, choose.value.option)) : value
}

function setOption(value: unknown) {
  if (typeof value === 'string') {
    patch({ option: value as HassOption, choice: { type: 'next' } })
  }
}

function setPick(value: unknown) {
  if (value === 'next' || value === 'previous') {
    patch({ choice: { type: value } })
  }
  else if (value === 'set') {
    patch({ choice: { type: 'set', option: choices.value[0] ?? '' } })
  }
}

function setChoice(value: unknown) {
  if (typeof value === 'string') {
    patch({ choice: { type: 'set', option: value } })
  }
}

const color = computed({
  get: () => (command.value.kind === 'color' ? toHex(command.value.color) : '#ffffff'),
  set: (hex: string) => patch({ color: fromHex(hex) }),
})

const agents = computed(() => hass.entities.value.filter(e => domainOf(e.id) === 'conversation'))
const NO_AGENT = '-'

const eventData = ref('')
const eventError = ref(false)
watch(() => (command.value.kind === 'event' ? command.value.data : null), (data) => {
  if (data && !eventError.value) {
    eventData.value = Object.keys(data).length ? JSON.stringify(data, null, 2) : ''
  }
}, { immediate: true })

function setEventData(text: string) {
  eventData.value = text
  if (!text.trim()) {
    eventError.value = false
    patch({ data: {} })
    return
  }
  try {
    const parsed = JSON.parse(text) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error('not an object')
    }
    eventError.value = false
    patch({ data: parsed })
  }
  catch {
    eventError.value = true
  }
}

/** Alarm panels and locks with a code take it from the secrets. */
const needsCode = computed(() => {
  const domain = domainOf(entityId.value)
  return (domain === 'alarm_control_panel' || domain === 'lock') && ['switch', 'run', 'alarm'].includes(command.value.kind)
})

</script>

<template>
  <div class="flex flex-col gap-2">
    <template v-if="command.kind !== 'assist' && command.kind !== 'event' && command.kind !== 'call'">
      <HassEntityField :model-value="entityId" :domains="domains" @update:model-value="setEntity" />
      <p v-if="!switchable" class="text-[11px] text-destructive">
        {{ t('actions.hass.cannotSwitch') }}
      </p>
    </template>

    <template v-if="command.kind === 'switch' && entityId">
      <ToggleGroup v-if="labels" :model-value="command.mode" type="single" variant="outline" size="sm" class="w-full" :aria-label="t('actions.hass.whatItDoes')" @update:model-value="setMode">
        <ToggleGroupItem value="toggle" class="flex-1">
          {{ t('actions.hass.toggle') }}
        </ToggleGroupItem>
        <ToggleGroupItem value="on" class="flex-1">
          {{ labels.on }}
        </ToggleGroupItem>
        <ToggleGroupItem value="off" class="flex-1">
          {{ labels.off }}
        </ToggleGroupItem>
      </ToggleGroup>
      <p v-else class="text-[11px] text-muted-foreground">
        {{ t('actions.hass.onlyRuns') }}
      </p>
    </template>

    <p v-else-if="command.kind === 'run'" class="text-[11px] text-muted-foreground">
      {{ t('actions.hass.runHint') }}
    </p>

    <template v-else-if="level && entityId">
      <Select :model-value="level.level" @update:model-value="setLevel">
        <SelectTrigger class="w-full" :aria-label="t('actions.hass.level')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="l in levels" :key="l" :value="l">
            {{ t(`actions.hass.levels.${camel(l)}`) }}
          </SelectItem>
        </SelectContent>
      </Select>
      <div class="flex items-center gap-2">
        <ToggleGroup :model-value="way" type="single" variant="outline" size="sm" class="flex-1" :aria-label="t('actions.hass.way')" @update:model-value="setWay">
          <ToggleGroupItem value="up" class="flex-1">
            {{ t('actions.hass.up') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="down" class="flex-1">
            {{ t('actions.hass.down') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="set" class="flex-1">
            {{ t('actions.hass.set') }}
          </ToggleGroupItem>
        </ToggleGroup>
        <Input
          :model-value="amount"
          type="number"
          :step="level.level === 'temperature' ? 0.5 : 1"
          class="h-8 w-24 text-right"
          :aria-label="t('actions.hass.amount')"
          @update:model-value="setChange(way, Number($event))"
        />
        <span class="w-6 text-xs text-muted-foreground">{{ unit }}</span>
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t(way === 'set' ? 'actions.hass.setHint' : 'actions.hass.stepHint') }}
      </p>
    </template>

    <template v-else-if="choose && entityId">
      <Select :model-value="choose.option" @update:model-value="setOption">
        <SelectTrigger class="w-full" :aria-label="t('actions.hass.optionKind')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="o in options" :key="o" :value="o">
            {{ t(`actions.hass.options.${camel(o)}`) }}
          </SelectItem>
        </SelectContent>
      </Select>
      <ToggleGroup :model-value="choose.choice.type" type="single" variant="outline" size="sm" class="w-full" :aria-label="t('actions.hass.whichOption')" @update:model-value="setPick">
        <ToggleGroupItem value="next" class="flex-1">
          {{ t('actions.hass.next') }}
        </ToggleGroupItem>
        <ToggleGroupItem value="previous" class="flex-1">
          {{ t('actions.hass.previous') }}
        </ToggleGroupItem>
        <ToggleGroupItem value="set" class="flex-1">
          {{ t('actions.hass.this') }}
        </ToggleGroupItem>
      </ToggleGroup>
      <template v-if="choose.choice.type === 'set'">
        <Select v-if="choices.length" :model-value="choose.choice.option" @update:model-value="setChoice">
          <SelectTrigger class="w-full" :aria-label="t('actions.hass.option')">
            <SelectValue :placeholder="t('actions.hass.chooseOption')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="c in choices" :key="c" :value="c">
              {{ choiceText(c) }}
            </SelectItem>
          </SelectContent>
        </Select>
        <Input v-else :model-value="choose.choice.option" :placeholder="t('actions.hass.option')" :aria-label="t('actions.hass.option')" @update:model-value="setChoice(String($event))" />
      </template>
    </template>

    <Select v-else-if="command.kind === 'media'" :model-value="command.control" @update:model-value="patch({ control: $event })">
      <SelectTrigger class="w-full" :aria-label="t('actions.hass.mediaControl')">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="m in MEDIA" :key="m" :value="m">
          {{ t(`actions.hass.media.${camel(m)}`) }}
        </SelectItem>
      </SelectContent>
    </Select>

    <div v-else-if="command.kind === 'color'" class="flex items-center gap-2 text-xs">
      <ColorPicker v-model="color" :label="t('actions.hass.color')" />
      <span class="text-muted-foreground">{{ t('actions.hass.colorHint') }}</span>
    </div>

    <Select v-else-if="command.kind === 'alarm'" :model-value="command.mode" @update:model-value="patch({ mode: $event })">
      <SelectTrigger class="w-full" :aria-label="t('actions.hass.alarmMode')">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="m in ALARM" :key="m" :value="m">
          {{ t(`actions.hass.alarm.${camel(m)}`) }}
        </SelectItem>
      </SelectContent>
    </Select>

    <template v-else-if="command.kind === 'assist'">
      <Textarea
        :model-value="command.text"
        rows="2"
        :placeholder="t('actions.hass.assistPlaceholder')"
        :aria-label="t('actions.hass.assistText')"
        @update:model-value="patch({ text: String($event) })"
      />
      <Select v-if="agents.length > 1" :model-value="command.agent || NO_AGENT" @update:model-value="patch({ agent: $event === NO_AGENT ? '' : $event })">
        <SelectTrigger class="w-full" :aria-label="t('actions.hass.agent')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem :value="NO_AGENT">
            {{ t('actions.hass.defaultAgent') }}
          </SelectItem>
          <SelectItem v-for="a in agents" :key="a.id" :value="a.id">
            {{ a.name }}
          </SelectItem>
        </SelectContent>
      </Select>
      <p class="text-[11px] text-muted-foreground">
        {{ t('actions.hass.assistHint') }}
      </p>
    </template>

    <template v-else-if="command.kind === 'event'">
      <Input
        :model-value="command.event"
        :placeholder="t('actions.hass.eventPlaceholder')"
        class="font-mono text-xs"
        :aria-label="t('actions.hass.eventType')"
        @update:model-value="patch({ event: String($event).trim() })"
      />
      <Textarea
        :model-value="eventData"
        rows="3"
        class="font-mono text-xs"
        placeholder="{ &quot;key&quot;: 1 }"
        :aria-label="t('actions.hass.eventData')"
        :aria-invalid="eventError"
        @update:model-value="setEventData(String($event))"
      />
      <p class="text-[11px]" :class="eventError ? 'text-destructive' : 'text-muted-foreground'">
        {{ eventError ? t('actions.hass.jsonError') : t('actions.hass.eventHint') }}
      </p>
    </template>

    <HassCodeField v-if="needsCode && entityId" :entity="entityId" />

    <p v-if="!hass.connected.value && hass.info.value" class="text-[11px] text-muted-foreground">
      {{ t('actions.hass.notConnected', { status: hassStatusText(hass.info.value) }) }}
      <NuxtLink to="/settings?tab=integrations" class="underline hover:text-foreground">
        {{ t('actions.hass.settingsLink') }}
      </NuxtLink>
    </p>
  </div>
</template>
