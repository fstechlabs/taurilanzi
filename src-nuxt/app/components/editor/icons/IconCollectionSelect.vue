<script setup lang="ts">
import { Check, ChevronsUpDown, Palette } from '@lucide/vue'
import type { IconCollection } from '~/lib/iconify'
import { ANY, categoryLabel } from '~/composables/useIconCatalog'

const prefix = defineModel<string>({ required: true })
const props = defineProps<{ collections: IconCollection[], categories: string[] }>()
const { t, locale } = useI18n()

const term = ref('')

const selected = computed(() => props.collections.find(c => c.prefix === prefix.value))

const groups = computed(() => {
  const needle = term.value.trim().toLowerCase()
  const matches = props.collections.filter(c =>
    !needle || c.name.toLowerCase().includes(needle) || c.prefix.includes(needle))
  return props.categories
    .map(category => ({
      category,
      items: matches.filter(c => (c.category ?? '') === category),
    }))
    .filter(group => group.items.length)
})

function count(total: number) {
  return total.toLocaleString(locale.value)
}
</script>

<template>
  <Combobox v-model="prefix" ignore-filter @update:open="open => open || (term = '')">
    <ComboboxAnchor as-child>
      <ComboboxTrigger as-child>
        <Button variant="outline" class="w-64 justify-between font-normal" :aria-label="t('icons.collections.label')">
          <span class="truncate">{{ selected?.name ?? t('icons.collections.all') }}</span>
          <ChevronsUpDown class="opacity-50" />
        </Button>
      </ComboboxTrigger>
    </ComboboxAnchor>
    <ComboboxList class="w-80" align="end">
      <ComboboxInput v-model="term" :placeholder="t('icons.collections.search')" :aria-label="t('icons.collections.searchLabel')" />
      <ComboboxViewport class="max-h-80">
        <ComboboxItem :value="ANY">
          {{ t('icons.collections.all') }}
          <ComboboxItemIndicator><Check /></ComboboxItemIndicator>
        </ComboboxItem>
        <ComboboxGroup v-for="group in groups" :key="group.category" :heading="categoryLabel(group.category || null)">
          <ComboboxItem v-for="c in group.items" :key="c.prefix" :value="c.prefix" :text-value="c.name">
            <span class="min-w-0 flex-1 truncate">{{ c.name }}</span>
            <Palette v-if="c.palette" class="text-muted-foreground" :aria-label="t('icons.collections.multicolor')" />
            <span class="text-xs text-muted-foreground tabular-nums">{{ count(c.total) }}</span>
            <ComboboxItemIndicator><Check /></ComboboxItemIndicator>
          </ComboboxItem>
        </ComboboxGroup>
        <p v-if="!groups.length" class="py-6 text-center text-sm text-muted-foreground">
          {{ t('icons.collections.noMatch') }}
        </p>
      </ComboboxViewport>
    </ComboboxList>
  </Combobox>
</template>
