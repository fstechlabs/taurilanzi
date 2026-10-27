<script setup lang="ts">
const KEY_COUNT = 13
const WIDE_KEY = 13
/** The D200 panel is 1220 px wide: 196 px keys separated by 60 px gaps, no outer margin. */
const GAP = 'calc(100cqw * 60 / 1220)'

const props = defineProps<{ page: string, selected?: number | null, compact?: boolean }>()
const emit = defineEmits<{ select: [index: number] }>()
defineSlots<{
  header?: () => unknown
  key?: (props: { index: number }) => unknown
}>()

const { snapshot, pressed, keyImageUrl } = useEngine()

const keys = computed(() => [...Array(KEY_COUNT).keys(), WIDE_KEY])
const page = computed(() => snapshot.value?.pages.find(p => p.id === props.page))
const onDock = computed(() => snapshot.value?.path.at(-1) === props.page)

function configured(index: number) {
  return Boolean(page.value?.keys[index])
}
</script>

<template>
  <div
    class="relative w-full bg-black shadow-xl ring-1 ring-white/10"
    :class="props.compact ? 'rounded-2xl px-[4%] pt-[2.5%] pb-[4%]' : 'rounded-2xl p-[4%]'"
  >
    <slot name="header" />
    <div class="@container">
      <div class="grid grid-cols-5" :style="{ gap: GAP }">
        <DockKey
          v-for="index in keys"
          :key="index"
          :page="props.page"
          :index="index"
          :wide="index === WIDE_KEY"
          :image-url="keyImageUrl(props.page, index)"
          :configured="configured(index)"
          :pressed="onDock && pressed.has(index)"
          :selected="props.selected === index"
          :compact="props.compact"
          @select="emit('select', $event)"
        >
          <slot name="key" :index="index" />
        </DockKey>
      </div>
    </div>
  </div>
</template>
