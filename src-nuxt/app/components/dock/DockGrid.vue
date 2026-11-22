<script setup lang="ts">
import { keyAspect, placement } from '~/lib/dock'

const props = defineProps<{ page: string, selected?: number | null, compact?: boolean }>()
const emit = defineEmits<{ select: [index: number] }>()
defineSlots<{
  header?: () => unknown
  key?: (props: { index: number }) => unknown
}>()

const { snapshot, pressed, keyImageUrl, model } = useEngine()

const page = computed(() => snapshot.value?.pages.find(p => p.id === props.page))
const onDock = computed(() => snapshot.value?.path.at(-1) === props.page)
/** The face of the dock the editor shows, every control where it sits on it. */
const face = computed(() => model.value ? `${model.value.panel.width} / ${model.value.panel.height}` : '1220 / 708')

function configured(index: number) {
  return Boolean(page.value?.keys[index])
}

function unsupported(index: number) {
  return Boolean(snapshot.value?.unsupported.some(s => s.page === props.page && s.index === index))
}
</script>

<template>
  <div
    class="relative w-full bg-black shadow-xl ring-1 ring-white/10"
    :class="props.compact ? 'rounded-2xl px-[4%] pt-[2.5%] pb-[4%]' : 'rounded-2xl p-[4%]'"
  >
    <slot name="header" />
    <div v-if="model" class="relative w-full" :style="{ aspectRatio: face }" :data-dock-model="model.id">
      <DockKey
        v-for="control in model.controls"
        :key="control.index"
        :page="props.page"
        :index="control.index"
        :aspect="keyAspect(model, control.index)"
        :style="placement(model, control)"
        :image-url="keyImageUrl(props.page, control.index)"
        :configured="configured(control.index)"
        :unsupported="unsupported(control.index)"
        :pressed="onDock && pressed.has(control.index)"
        :selected="props.selected === control.index"
        :compact="props.compact"
        @select="emit('select', $event)"
      >
        <slot name="key" :index="control.index" />
      </DockKey>
    </div>
  </div>
</template>
