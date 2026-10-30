<script setup lang="ts">
import { Plus, X } from '@lucide/vue'
import type { StateDraft } from '~/composables/useKeyEditor'
import { widgetsOf } from '~/composables/useKeyEditor'
import type { ClockLine, Metric, Period, Widget } from '~/types/engine'
import { fromHex, toHex } from '~/lib/color'
import type { WidgetKind } from '~/lib/widgets'
import { HASS_WIDGET_KINDS, METRICS, PERIODS, PRESSED_KINDS, WIDGET_KINDS, WORLD_CLOCKS_MAX, newWidget, nextKind, widgetLabel, zoneCity } from '~/lib/widgets'
import { isDefaultKeyText } from '~/lib/new-key'
import { os } from '~/lib/os'

const draft = defineModel<StateDraft>({ required: true })
const { fonts } = useFonts()
const { t } = useI18n()

const NONE = 'none'
const LINES: { value: ClockLine, label: string }[] = [
  { value: 'none', label: 'widgets.lines.none' },
  { value: 'date', label: 'widgets.lines.date' },
  { value: 'weekday', label: 'widgets.lines.weekday' },
  { value: 'weekday_date', label: 'widgets.lines.weekdayDate' },
]

/** Which of the widgets of the look is being edited. */
const shown = defineModel<number>('shown', { default: 0 })

const widgets = computed(() => widgetsOf(draft.value))
const current = computed(() => widgets.value[shown.value] ?? widgets.value[0] ?? null)
const several = computed(() => widgets.value.length > 1)
const canAdd = computed(() => widgets.value.length > 0 && !widgets.value.some(w => PRESSED_KINDS.includes(w.kind)))

watch(() => widgets.value.length, (count) => {
  if (shown.value >= count) {
    shown.value = Math.max(0, count - 1)
  }
})

function put(index: number, widget: Widget) {
  const state = draft.value
  if (index === 0 || !state.widget) {
    state.widget = widget
  }
  else {
    state.moreWidgets[index - 1] = widget
  }
}

function remove(index: number) {
  const state = draft.value
  if (index === 0) {
    state.widget = state.moreWidgets.shift() ?? null
  }
  else {
    state.moreWidgets.splice(index - 1, 1)
  }
  shown.value = Math.min(index, Math.max(0, widgets.value.length - 1))
}

function add() {
  const widget = current.value
  draft.value.moreWidgets.push(newWidget(nextKind(widgets.value), widget))
  shown.value = widgets.value.length - 1
}

const kind = computed({
  get: () => current.value?.kind ?? NONE,
  set: (value: string) => {
    const state = draft.value
    const widget = current.value
    if (value === NONE) {
      remove(widget ? shown.value : 0)
      return
    }
    if (value === widget?.kind) {
      return
    }
    // The text a key is created with would only get in the way of the widget.
    if (!state.widget && isDefaultKeyText(state.text?.content)) {
      state.text = null
    }
    put(widget ? shown.value : 0, newWidget(value as WidgetKind, widget))
  },
})

/** A key of several widgets cannot hold one that answers to presses. */
function unavailable(option: WidgetKind) {
  return several.value && PRESSED_KINDS.includes(option)
}

type Of<K extends WidgetKind> = Extract<Widget, { kind: K }>
function only<K extends WidgetKind>(wanted: K) {
  return computed(() => (current.value?.kind === wanted ? current.value as Of<K> : null))
}
const digital = only('digital_clock')
const analog = only('analog_clock')
const monitor = only('system_monitor')
const network = only('network')
const countdown = only('countdown')
const world = only('world_clocks')
const timer = only('timer')
const calendar = only('calendar')
const battery = only('battery')
const playing = only('now_playing')
const temperatures = only('temperatures')
const processes = only('top_processes')
const latency = only('latency')
const volume = only('volume')
const moon = only('moon')
const progress = only('progress')
const stopwatch = only('stopwatch')
const counter = only('counter')
const words = only('word_clock')
const phrases = only('phrases')

function setPeriods(value: unknown) {
  const chosen = value as Period[]
  if (progress.value && chosen.length) {
    progress.value.periods = PERIODS.map(p => p.period).filter(p => chosen.includes(p))
  }
}

const usage = computed({
  get: () => processes.value?.by ?? 'cpu',
  set: (value: string) => {
    if (processes.value && (value === 'cpu' || value === 'memory')) {
      processes.value.by = value
    }
  },
})

function setStep(value: string | number) {
  const step = Math.round(Number(value))
  if (counter.value && Number.isFinite(step)) {
    counter.value.step = Math.max(-1000, Math.min(1000, step))
  }
}

/** One phrase per line. */
const phraseText = computed({
  get: () => phrases.value?.phrases.join('\n') ?? '',
  set: (text: string) => {
    if (phrases.value) {
      phrases.value.phrases = text.split('\n')
    }
  },
})

function setPhraseSeconds(value: string | number) {
  const seconds = Math.round(Number(value))
  if (phrases.value && Number.isFinite(seconds)) {
    phrases.value.seconds = Math.max(0, Math.min(24 * 3600, seconds))
  }
}

const worldHours = computed({
  get: () => (world.value?.hour12 ? '12' : '24'),
  set: (value: string) => {
    if (world.value) {
      world.value.hour12 = value === '12'
    }
  },
})

function addCity() {
  if (world.value && world.value.clocks.length < WORLD_CLOCKS_MAX) {
    world.value.clocks.push({ zone: 'Europe/London', label: '' })
  }
}

function setCity(index: number, zone: string | null) {
  const clock = world.value?.clocks[index]
  if (clock && zone) {
    clock.zone = zone
  }
}

/** The length of the timer, split for its two fields. */
const timerMinutes = computed(() => Math.floor((timer.value?.seconds ?? 0) / 60))
const timerSeconds = computed(() => (timer.value?.seconds ?? 0) % 60)
/** Up to ten hours, never zero. */
function setTimer(minutes: number, seconds: number) {
  if (!timer.value || !Number.isFinite(minutes) || !Number.isFinite(seconds)) {
    return
  }
  const total = Math.round(Math.max(0, minutes)) * 60 + Math.round(Math.min(59, Math.max(0, seconds)))
  timer.value.seconds = Math.min(10 * 3600, Math.max(1, total))
}
/** Monitor and network can show their recent history as a graph. */
const graphable = computed(() => monitor.value ?? network.value)
/** The app keeps a day of readings. */
const MAX_MINUTES = 24 * 60

function setMinutes(value: string | number) {
  const minutes = Math.round(Number(value))
  if (graphable.value && Number.isFinite(minutes) && minutes >= 1) {
    graphable.value.minutes = Math.min(MAX_MINUTES, minutes)
  }
}

/** "90" reads as "1 h 30 min", to check long periods at a glance. */
function duration(minutes: number) {
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  if (!hours) {
    return `${rest} min`
  }
  return rest ? `${hours} h ${rest} min` : `${hours} h`
}
const view = computed({
  get: () => (graphable.value?.graph ? 'graph' : 'now'),
  set: (value) => {
    if (graphable.value && value) {
      graphable.value.graph = value === 'graph'
    }
  },
})
const clock = computed(() => digital.value ?? analog.value)

/** Choosing a city names the clock after it, unless it already has a name of its own. */
const zone = computed({
  get: () => clock.value?.zone ?? null,
  set: (next: string | null) => {
    const widget = clock.value
    if (!widget) {
      return
    }
    const named = widget.zone ? zoneCity(widget.zone) : ''
    if (!widget.label || widget.label === named) {
      widget.label = next ? zoneCity(next) : ''
    }
    widget.zone = next
  },
})

const hours = computed({
  get: () => (digital.value?.hour12 ? '12' : '24'),
  set: (value: string) => {
    if (digital.value) {
      digital.value.hour12 = value === '12'
    }
  },
})

function setMetrics(value: unknown) {
  const chosen = value as Metric[]
  if (monitor.value && chosen.length) {
    monitor.value.metrics = METRICS.map(m => m.metric).filter(m => chosen.includes(m))
  }
}

const color = computed({
  get: () => toHex(current.value?.style.color ?? { r: 255, g: 255, b: 255 }),
  set: (hex: string) => {
    if (current.value) {
      current.value.style.color = fromHex(hex)
    }
  },
})
const accent = computed({
  get: () => toHex(current.value?.style.accent ?? { r: 255, g: 255, b: 255 }),
  set: (hex: string) => {
    if (current.value) {
      current.value.style.accent = fromHex(hex)
    }
  },
})
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.show') }}</Label>
      <div v-if="several" class="flex flex-wrap gap-1.5" role="tablist" :aria-label="t('widgets.fields.list')">
        <div
          v-for="(w, i) in widgets"
          :key="i"
          class="flex items-center rounded-md border text-xs"
          :class="i === shown ? 'border-sky-500/60 bg-sky-500/15' : 'border-border'"
          data-widget-tab
        >
          <button type="button" role="tab" :aria-selected="i === shown" class="py-1 pl-2 pr-1" @click="shown = i">
            {{ i + 1 }}. {{ widgetLabel(w.kind) }}
          </button>
          <button
            type="button"
            class="py-1 pl-0.5 pr-1.5 text-muted-foreground hover:text-foreground"
            :aria-label="t('widgets.fields.removeWidget', { name: widgetLabel(w.kind) })"
            @click="remove(i)"
          >
            <X class="size-3" />
          </button>
        </div>
      </div>
      <Select v-model="kind">
        <SelectTrigger class="w-full" :aria-label="t('widgets.fields.live')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem :value="NONE">
            {{ t('widgets.fields.none') }}
          </SelectItem>
          <SelectItem v-for="k in WIDGET_KINDS" :key="k.kind" :value="k.kind" :disabled="unavailable(k.kind)">
            {{ k.label }}
          </SelectItem>
          <SelectSeparator />
          <SelectGroup>
            <SelectLabel>Home Assistant</SelectLabel>
            <SelectItem v-for="k in HASS_WIDGET_KINDS" :key="k.kind" :value="k.kind">
              {{ k.label }}
            </SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.hint') }}
      </p>
      <p v-if="several" class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.several') }}
      </p>
      <Button v-if="canAdd" variant="outline" size="sm" :aria-label="t('widgets.fields.addLabel')" @click="add">
        <Plus /> {{ t('widgets.fields.add') }}
      </Button>
      <p v-else-if="current && PRESSED_KINDS.includes(current.kind)" class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.alone') }}
      </p>
    </div>

    <template v-if="digital">
      <label class="flex items-center justify-between gap-2 text-sm">
        {{ t('widgets.fields.seconds') }}
        <Switch v-model="digital.seconds" :aria-label="t('widgets.fields.showSeconds')" />
      </label>
      <div class="flex items-center gap-3">
        <span class="w-14 text-xs text-muted-foreground">{{ t('widgets.fields.format') }}</span>
        <ToggleGroup :model-value="hours" type="single" variant="outline" size="sm" class="flex-1" @update:model-value="v => v && (hours = v as string)">
          <ToggleGroupItem value="24" class="flex-1">
            24 h
          </ToggleGroupItem>
          <ToggleGroupItem value="12" class="flex-1">
            12 h
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.secondLine') }}</Label>
        <Select v-model="digital.second_line">
          <SelectTrigger class="w-full" :aria-label="t('widgets.fields.secondLine')">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="line in LINES" :key="line.value" :value="line.value">
              {{ t(line.label) }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
    </template>

    <template v-if="analog">
      <label class="flex items-center justify-between gap-2 text-sm">
        {{ t('widgets.fields.secondHand') }}
        <Switch v-model="analog.seconds" :aria-label="t('widgets.fields.showSecondHand')" />
      </label>
      <label class="flex items-center justify-between gap-2 text-sm">
        <span>{{ t('widgets.fields.dateBeside') }} <span class="block text-[11px] text-muted-foreground">{{ t('widgets.fields.dateBesideHint') }}</span></span>
        <Switch v-model="analog.date" :aria-label="t('widgets.fields.showDate')" />
      </label>
    </template>

    <template v-if="clock">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.timeZone') }}</Label>
        <TimeZoneField v-model="zone" />
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
        <Input v-model="clock.label" :placeholder="t('widgets.fields.clockPlaceholder')" maxlength="24" :aria-label="t('widgets.fields.clockLabel')" />
      </div>
    </template>

    <div v-if="monitor" class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.metrics') }}</Label>
      <ToggleGroup :model-value="monitor.metrics" type="multiple" variant="outline" size="sm" class="w-full" @update:model-value="setMetrics">
        <ToggleGroupItem v-for="m in METRICS" :key="m.metric" :value="m.metric" class="flex-1">
          {{ m.label }}
        </ToggleGroupItem>
      </ToggleGroup>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.diskHint') }}
      </p>
    </div>

    <p v-if="network" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.networkHint') }}
    </p>

    <div v-if="graphable" class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.view') }}</Label>
      <ToggleGroup v-model="view" type="single" variant="outline" size="sm" class="w-full" :aria-label="t('widgets.fields.view')">
        <ToggleGroupItem value="now" class="flex-1">
          {{ monitor ? t('widgets.fields.bars') : t('widgets.fields.figures') }}
        </ToggleGroupItem>
        <ToggleGroupItem value="graph" class="flex-1">
          {{ t('widgets.fields.graph') }}
        </ToggleGroupItem>
      </ToggleGroup>
      <template v-if="graphable.graph">
        <div class="flex items-center gap-2">
          <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.period') }}</Label>
          <Input
            :model-value="graphable.minutes ?? 5"
            type="number"
            min="1"
            :max="MAX_MINUTES"
            step="1"
            class="h-8 w-24 text-right"
            :aria-label="t('widgets.fields.graphMinutes')"
            @update:model-value="setMinutes"
          />
          <span class="text-xs text-muted-foreground">{{ t('widgets.fields.minutesUnit') }}</span>
          <span v-if="(graphable.minutes ?? 5) >= 60" class="text-xs text-muted-foreground">· {{ duration(graphable.minutes ?? 5) }}</span>
        </div>
        <p class="text-[11px] text-muted-foreground">
          {{ network ? `${t('widgets.fields.graphNetworkHint')} ${t('widgets.fields.graphHint')}` : t('widgets.fields.graphHint') }}
        </p>
      </template>
    </div>

    <template v-if="countdown">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.until') }}</Label>
        <Input v-model="countdown.target" type="datetime-local" :aria-label="t('widgets.fields.countdownTarget')" />
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
        <Input v-model="countdown.label" :placeholder="t('widgets.fields.countdownPlaceholder')" maxlength="24" :aria-label="t('widgets.fields.countdownLabel')" />
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.whenReached') }}</Label>
        <Input v-model="countdown.done" :placeholder="t('widgets.fields.donePlaceholder')" maxlength="24" :aria-label="t('widgets.fields.whenReachedLabel')" />
      </div>
    </template>

    <template v-if="world">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.cities') }}</Label>
        <div v-for="(city, i) in world.clocks" :key="i" class="flex flex-col gap-1.5 rounded-md bg-neutral-900/60 p-2" data-world-clock>
          <div class="flex items-center gap-1.5">
            <div class="min-w-0 flex-1">
              <TimeZoneField :model-value="city.zone" @update:model-value="setCity(i, $event)" />
            </div>
            <Button variant="ghost" size="icon" class="size-8 shrink-0" :aria-label="t('widgets.fields.removeCity', { city: zoneCity(city.zone) })" :disabled="world.clocks.length < 2" @click="world.clocks.splice(i, 1)">
              <X />
            </Button>
          </div>
          <Input v-model="city.label" :placeholder="zoneCity(city.zone)" maxlength="16" :aria-label="t('widgets.fields.cityName', { city: zoneCity(city.zone) })" />
        </div>
        <Button v-if="world.clocks.length < WORLD_CLOCKS_MAX" variant="outline" size="sm" @click="addCity">
          <Plus /> {{ t('widgets.fields.addCity') }}
        </Button>
        <p class="text-[11px] text-muted-foreground">
          {{ t('widgets.fields.citiesHint', { max: WORLD_CLOCKS_MAX }) }}
        </p>
      </div>
      <div class="flex items-center gap-3">
        <span class="w-14 text-xs text-muted-foreground">{{ t('widgets.fields.format') }}</span>
        <ToggleGroup :model-value="worldHours" type="single" variant="outline" size="sm" class="flex-1" @update:model-value="v => v && (worldHours = v as string)">
          <ToggleGroupItem value="24" class="flex-1">
            24 h
          </ToggleGroupItem>
          <ToggleGroupItem value="12" class="flex-1">
            12 h
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
    </template>

    <p v-if="calendar" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.calendarHint') }}
    </p>

    <template v-if="timer">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.duration') }}</Label>
        <div class="flex items-center gap-2">
          <Input :model-value="timerMinutes" type="number" min="0" max="600" class="h-8 w-20 text-right" :aria-label="t('widgets.fields.timerMinutes')" @update:model-value="v => setTimer(Number(v), timerSeconds)" />
          <span class="text-xs text-muted-foreground">min</span>
          <Input :model-value="timerSeconds" type="number" min="0" max="59" class="h-8 w-20 text-right" :aria-label="t('widgets.fields.timerSeconds')" @update:model-value="v => setTimer(timerMinutes, Number(v))" />
          <span class="text-xs text-muted-foreground">s</span>
        </div>
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
        <Input v-model="timer.label" :placeholder="t('widgets.fields.timerPlaceholder')" maxlength="24" :aria-label="t('widgets.fields.timerLabel')" />
      </div>
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.whenDone') }}</Label>
        <Input v-model="timer.done" :placeholder="t('widgets.fields.timeUpPlaceholder')" maxlength="24" :aria-label="t('widgets.fields.whenDoneLabel')" />
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.timerHint') }}
      </p>
    </template>

    <p v-if="battery" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.batteryHint') }}
    </p>

    <p v-if="playing" class="text-[11px] text-muted-foreground">
      {{ t(`widgets.fields.nowPlayingHint.${os}`) }}
    </p>

    <p v-if="temperatures" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.temperaturesHint') }}
    </p>

    <template v-if="processes">
      <div class="flex items-center gap-3">
        <span class="w-14 text-xs text-muted-foreground">{{ t('widgets.fields.by') }}</span>
        <ToggleGroup :model-value="usage" type="single" variant="outline" size="sm" class="flex-1" :aria-label="t('widgets.fields.sortBy')" @update:model-value="v => v && (usage = v as string)">
          <ToggleGroupItem value="cpu" class="flex-1">
            CPU
          </ToggleGroupItem>
          <ToggleGroupItem value="memory" class="flex-1">
            {{ t('widgets.fields.memory') }}
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.processesHint') }}
      </p>
    </template>

    <template v-if="latency">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.host') }}</Label>
        <Input v-model="latency.host" :placeholder="t('widgets.fields.hostPlaceholder')" maxlength="253" :aria-label="t('widgets.fields.hostLabel')" />
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.latencyHint') }}
      </p>
    </template>

    <p v-if="volume" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.volumeHint') }}
    </p>

    <p v-if="moon" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.moonHint') }}
    </p>

    <div v-if="progress" class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.periods') }}</Label>
      <ToggleGroup :model-value="progress.periods" type="multiple" variant="outline" size="sm" class="w-full" @update:model-value="setPeriods">
        <ToggleGroupItem v-for="p in PERIODS" :key="p.period" :value="p.period" class="flex-1">
          {{ p.label }}
        </ToggleGroupItem>
      </ToggleGroup>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.progressHint') }}
      </p>
    </div>

    <template v-if="stopwatch">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
        <Input v-model="stopwatch.label" :placeholder="t('widgets.fields.stopwatchPlaceholder')" maxlength="24" :aria-label="t('widgets.fields.stopwatchLabel')" />
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.stopwatchHint') }}
      </p>
    </template>

    <template v-if="counter">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.label') }}</Label>
        <Input v-model="counter.label" :placeholder="t('widgets.fields.counterPlaceholder')" maxlength="24" :aria-label="t('widgets.fields.counterLabel')" />
      </div>
      <div class="flex items-center gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.step') }}</Label>
        <Input :model-value="counter.step" type="number" min="-1000" max="1000" class="h-8 w-20 text-right" :aria-label="t('widgets.fields.stepLabel')" @update:model-value="setStep" />
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.counterHint') }}
      </p>
    </template>

    <p v-if="words" class="text-[11px] text-muted-foreground">
      {{ t('widgets.fields.wordClockHint') }}
    </p>

    <template v-if="phrases">
      <div class="flex flex-col gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.phrases') }}</Label>
        <Textarea v-model="phraseText" rows="4" class="text-sm" :aria-label="t('widgets.fields.phrasesLabel')" />
      </div>
      <div class="flex items-center gap-2">
        <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.changeEvery') }}</Label>
        <Input :model-value="phrases.seconds" type="number" min="0" class="h-8 w-20 text-right" :aria-label="t('widgets.fields.phraseSeconds')" @update:model-value="setPhraseSeconds" />
        <span class="text-xs text-muted-foreground">{{ t('widgets.fields.secondsUnit') }}</span>
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t('widgets.fields.phrasesHint') }}
      </p>
    </template>

    <HassWidgetFields v-if="current?.kind.startsWith('hass_')" :widget="current" />

    <div v-if="current" class="flex flex-col gap-2">
      <Label class="text-xs text-muted-foreground">{{ t('widgets.fields.style') }}</Label>
      <Select v-model="current.style.font_family">
        <SelectTrigger class="w-full" :aria-label="t('widgets.fields.font')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent class="max-h-72">
          <SelectItem v-for="font in fonts" :key="font" :value="font">
            {{ font }}
          </SelectItem>
        </SelectContent>
      </Select>
      <div class="flex gap-2">
        <ColorPicker v-model="color" :label="t('widgets.fields.textColor')" />
        <ColorPicker v-model="accent" :label="t('widgets.fields.accentColor')" />
      </div>
    </div>
  </div>
</template>
