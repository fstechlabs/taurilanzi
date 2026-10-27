<script setup lang="ts">
import { Check, ChevronsUpDown } from '@lucide/vue'
import { domainOf, isEntityId } from '~/lib/actions/hass'
import { normalize } from '~/lib/actions/catalog'
import { preload, thumbnail } from '~/lib/iconify'

/** An entity of Home Assistant by id: picked from its list, or typed while it is not connected. */
const entity = defineModel<string>({ required: true })
const props = defineProps<{
  /** Only entities of these domains; all of them when missing. */
  domains?: string[] | null
  label?: string
}>()

const { t } = useI18n()
const hass = useHass()
const term = ref('')
const iconsLoaded = ref(0)
const SHOWN = 80

onMounted(() => hass.load())

const pool = computed(() => hass.entities.value.filter(e => !props.domains || props.domains.includes(domainOf(e.id))))

const matches = computed(() => {
  const words = normalize(term.value).split(/\s+/).filter(Boolean)
  const found = words.length
    ? pool.value.filter((e) => {
        const haystack = normalize(`${e.name} ${e.id} ${hass.areaName(e.area) ?? ''} ${hass.domainName(domainOf(e.id))}`)
        return words.every(word => haystack.includes(word))
      })
    : pool.value.filter(e => !e.secondary)
  return found.slice(0, SHOWN)
})

/** What is typed, if it is an entity id the list does not have. */
const typed = computed(() => {
  const value = term.value.trim()
  return isEntityId(value) && !hass.entity(value) ? value : null
})

const current = computed(() => (entity.value ? hass.entity(entity.value) : undefined))

watch(matches, async (list) => {
  await preload([...list.map(e => hass.entityIcon(e.id)), current.value ? hass.entityIcon(current.value.id) : ''].filter(Boolean))
  iconsLoaded.value++
}, { immediate: true })

function icon(id: string) {
  void iconsLoaded.value
  return thumbnail(hass.entityIcon(id))
}

const selected = computed({
  get: () => entity.value,
  set: (value: string) => {
    if (value) {
      entity.value = value
    }
  },
})

function opened(open: boolean) {
  if (open) {
    hass.load()
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
        <Button variant="outline" class="h-auto min-h-9 w-full justify-between py-1.5 font-normal" :aria-label="props.label ?? t('actions.hass.entity')" data-hass-entity>
          <span class="flex min-w-0 items-center gap-2">
            <img v-if="entity && icon(entity)" :src="icon(entity)!" alt="" class="size-4 shrink-0">
            <span v-if="current" class="min-w-0 truncate text-left">
              {{ current.name }}
              <span class="block truncate text-[11px] text-muted-foreground">{{ current.id }}</span>
            </span>
            <span v-else-if="entity" class="truncate font-mono text-xs">{{ entity }}</span>
            <span v-else class="text-muted-foreground">{{ props.label ?? t('actions.hass.chooseEntity') }}</span>
          </span>
          <ChevronsUpDown class="opacity-50" />
        </Button>
      </ComboboxTrigger>
    </ComboboxAnchor>
    <ComboboxList class="w-96" align="start">
      <ComboboxInput v-model="term" :placeholder="t('actions.hass.searchEntity')" :aria-label="t('actions.hass.searchEntity')" />
      <ComboboxViewport class="max-h-80">
        <ComboboxItem v-if="typed" :value="typed" :text-value="typed">
          <span class="truncate">{{ t('actions.hass.useTyped', { id: typed }) }}</span>
        </ComboboxItem>
        <ComboboxItem v-for="e in matches" :key="e.id" :value="e.id" :text-value="e.name" :data-entity="e.id">
          <img v-if="icon(e.id)" :src="icon(e.id)!" alt="" class="size-4 shrink-0">
          <span v-else class="size-4 shrink-0" />
          <span class="min-w-0 flex-1">
            <span class="block truncate">{{ e.name }}</span>
            <span class="block truncate text-[11px] text-muted-foreground">
              {{ e.id }}<template v-if="hass.areaName(e.area)"> · {{ hass.areaName(e.area) }}</template>
            </span>
          </span>
          <span class="max-w-28 shrink-0 truncate text-xs text-muted-foreground">{{ hass.stateText(e.id) }}</span>
          <ComboboxItemIndicator><Check /></ComboboxItemIndicator>
        </ComboboxItem>
        <p v-if="!matches.length && !typed" class="px-3 py-6 text-center text-sm text-muted-foreground">
          {{ hass.connected.value ? (hass.loading.value ? t('actions.hass.reading') : t('actions.hass.noEntities')) : t('actions.hass.typeId') }}
        </p>
      </ComboboxViewport>
    </ComboboxList>
  </Combobox>
</template>
