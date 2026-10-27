<script setup lang="ts">
const minutes = defineModel<number>({ required: true })
const props = defineProps<{ label: string, options: number[] }>()
const { t } = useI18n()

function describe(value: number) {
  if (value === 0) {
    return t('settings.minutes.never')
  }
  return value % 60 === 0 ? `${value / 60} h` : `${value} min`
}

/** A value saved before the options changed still shows. */
const choices = computed(() => [...new Set([...props.options, minutes.value])].sort((a, b) => a - b))
const model = computed({
  get: () => String(minutes.value),
  set: (value) => { minutes.value = Number(value) },
})
</script>

<template>
  <Select v-model="model">
    <SelectTrigger class="w-36" :aria-label="label">
      <SelectValue />
    </SelectTrigger>
    <SelectContent>
      <SelectItem v-for="value in choices" :key="value" :value="String(value)">
        {{ describe(value) }}
      </SelectItem>
    </SelectContent>
  </Select>
</template>
