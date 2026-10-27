<script setup lang="ts">
import { Check, ChevronsUpDown } from '@lucide/vue'
import { zoneCity } from '~/lib/widgets'

/** IANA name; null is the Mac's own time zone. */
const zone = defineModel<string | null>({ required: true })
const { t, locale } = useI18n()

const LOCAL = 'local'

interface Zone { id: string, city: string, region: string, offset: string, search: string }

const term = ref('')
const zones = shallowRef<Zone[]>([])

function plain(text: string) {
  return text.normalize('NFD').replace(/[\u0300-\u036F]/g, '').toLowerCase()
}

function offsetOf(id: string, now: Date) {
  const parts = new Intl.DateTimeFormat(locale.value, { timeZone: id, timeZoneName: 'shortOffset' }).formatToParts(now)
  return parts.find(part => part.type === 'timeZoneName')?.value ?? ''
}

/** Every zone the system knows, read when the list first opens. */
function load() {
  if (zones.value.length) {
    return
  }
  const now = new Date()
  zones.value = Intl.supportedValuesOf('timeZone').map((id) => {
    const city = zoneCity(id)
    const region = id.includes('/') ? id.split('/')[0]!.replaceAll('_', ' ') : ''
    return { id, city, region, offset: offsetOf(id, now), search: plain(`${id} ${city}`) }
  })
}

const matches = computed(() => {
  const needle = plain(term.value.trim())
  return needle ? zones.value.filter(z => z.search.includes(needle)) : zones.value
})

const selected = computed({
  get: () => zone.value ?? LOCAL,
  set: (value: string) => { zone.value = value === LOCAL ? null : value },
})

function opened(open: boolean) {
  if (open) {
    load()
  }
  else {
    term.value = ''
  }
}
</script>

<template>
  <Combobox v-model="selected" ignore-filter @update:open="opened">
    <ComboboxAnchor as-child>
      <ComboboxTrigger as-child>
        <Button variant="outline" class="w-full justify-between font-normal" :aria-label="t('editor.timeZone.label')">
          <span class="truncate">
            <template v-if="zone">{{ zoneCity(zone) }} <span class="text-muted-foreground">· {{ zone }}</span></template>
            <template v-else>{{ t('editor.timeZone.mac') }}</template>
          </span>
          <ChevronsUpDown class="opacity-50" />
        </Button>
      </ComboboxTrigger>
    </ComboboxAnchor>
    <ComboboxList class="w-80" align="start">
      <ComboboxInput v-model="term" :placeholder="t('editor.timeZone.search')" :aria-label="t('editor.timeZone.searchLabel')" />
      <ComboboxViewport class="max-h-72">
        <ComboboxItem v-if="!term.trim()" :value="LOCAL">
          {{ t('editor.timeZone.mac') }}
          <ComboboxItemIndicator><Check /></ComboboxItemIndicator>
        </ComboboxItem>
        <ComboboxItem v-for="z in matches" :key="z.id" :value="z.id" :text-value="z.city">
          <span class="min-w-0 flex-1 truncate">{{ z.city }} <span v-if="z.region" class="text-muted-foreground">· {{ z.region }}</span></span>
          <span class="text-xs text-muted-foreground tabular-nums">{{ z.offset }}</span>
          <ComboboxItemIndicator><Check /></ComboboxItemIndicator>
        </ComboboxItem>
        <p v-if="!matches.length" class="py-6 text-center text-sm text-muted-foreground">
          {{ t('editor.timeZone.noMatch') }}
        </p>
      </ComboboxViewport>
    </ComboboxList>
  </Combobox>
</template>
