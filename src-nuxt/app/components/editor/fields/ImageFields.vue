<script setup lang="ts">
import { Crop, ImagePlus, Search, Trash2, Undo2 } from '@lucide/vue'
import type { PickedIcon } from '~/composables/useIconCatalog'
import type { ImageDraft, StateDraft } from '~/composables/useKeyEditor'

const draft = defineModel<StateDraft>({ required: true })
const props = defineProps<{ wide: boolean }>()
const emit = defineEmits<{ upload: [file: File], pickIcon: [picked: PickedIcon] }>()
const { t } = useI18n()

const input = useTemplateRef<HTMLInputElement>('input')
const pickerOpen = ref(false)
const editorOpen = ref(false)
/** A photo was just chosen: open the editor as soon as it is uploaded. */
let editUpload = false

const inset = computed({
  get: () => [Math.round((draft.value.image?.inset ?? 0) * 100)],
  set: ([value]) => {
    if (draft.value.image) {
      draft.value.image.inset = (value ?? 0) / 100
    }
  },
})
const image = computed({
  get: () => draft.value.image as ImageDraft,
  set: (value) => { draft.value.image = value },
})
const label = computed(() => {
  const image = draft.value.image
  if (image?.icon) {
    return image.crop ? t('editor.image.editedIcon', { icon: image.icon }) : image.icon
  }
  return image?.crop ? t('editor.image.edited') : t('editor.image.own')
})
/** The renderer measures gradient angles from the x axis; CSS from the top. */
const background = computed(() => draft.value.background === 'solid'
  ? draft.value.from
  : `linear-gradient(${draft.value.angle + 90}deg, ${draft.value.from}, ${draft.value.to})`)

watch(() => draft.value.image?.source, (source) => {
  if (source && editUpload) {
    editUpload = false
    editorOpen.value = true
  }
})

function pick(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (file) {
    editUpload = true
    emit('upload', file)
  }
  ;(event.target as HTMLInputElement).value = ''
}

/** Any image can take one color over its shape; icons of the search start with the text's. */
function setTint(on: boolean) {
  if (draft.value.image) {
    draft.value.image.tint = on ? (draft.value.image.tint ?? draft.value.text?.color ?? '#ffffff') : null
  }
}

function useOriginal() {
  if (draft.value.image) {
    draft.value.image = { ...draft.value.image, asset: draft.value.image.source, crop: null }
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <Label class="text-xs text-muted-foreground">{{ t('editor.image.title') }}</Label>
    <input
      ref="input"
      type="file"
      accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml"
      class="hidden"
      @change="pick"
    >
    <div class="grid grid-cols-2 gap-2">
      <Button variant="outline" size="sm" @click="pickerOpen = true">
        <Search /> {{ t('editor.image.searchIcon') }}
      </Button>
      <Button variant="outline" size="sm" @click="input?.click()">
        <ImagePlus /> {{ t('editor.image.upload') }}
      </Button>
    </div>

    <template v-if="draft.image">
      <div class="flex items-center gap-2 rounded-md border px-3 py-2 text-xs">
        <span class="min-w-0 flex-1 truncate text-muted-foreground">
          {{ label }}
        </span>
        <Button variant="ghost" size="sm" class="h-7" @click="editorOpen = true">
          <Crop /> {{ t('editor.image.edit') }}
        </Button>
        <Button
          v-if="draft.image.crop"
          variant="ghost"
          size="icon-sm"
          :aria-label="t('editor.image.original')"
          :title="t('editor.image.original')"
          @click="useOriginal"
        >
          <Undo2 />
        </Button>
        <Button variant="ghost" size="icon-sm" :aria-label="t('editor.image.remove')" @click="draft.image = null">
          <Trash2 />
        </Button>
      </div>
      <div class="flex items-center gap-3">
        <span class="w-14 text-xs text-muted-foreground">{{ t('editor.image.color') }}</span>
        <ToggleGroup
          :model-value="draft.image.tint === null ? 'original' : 'tint'"
          type="single"
          variant="outline"
          size="sm"
          class="flex-1"
          :aria-label="t('editor.image.colors')"
          @update:model-value="v => v && setTint(v === 'tint')"
        >
          <ToggleGroupItem value="original" class="flex-1">
            {{ t('editor.image.originalColors') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="tint" class="flex-1">
            {{ t('editor.image.oneColor') }}
          </ToggleGroupItem>
        </ToggleGroup>
        <ColorPicker v-if="draft.image.tint !== null" v-model="draft.image.tint" :label="t('editor.image.iconColor')" />
      </div>
      <ToggleGroup
        v-if="!draft.image.crop"
        :model-value="draft.image.fit"
        type="single"
        variant="outline"
        size="sm"
        class="w-full"
        @update:model-value="v => v && draft.image && (draft.image.fit = v as ImageDraft['fit'])"
      >
        <ToggleGroupItem value="contain" class="flex-1">
          {{ t('editor.image.fit') }}
        </ToggleGroupItem>
        <ToggleGroupItem value="cover" class="flex-1">
          {{ t('editor.image.cover') }}
        </ToggleGroupItem>
      </ToggleGroup>
      <div class="flex items-center gap-3">
        <span class="w-14 text-xs text-muted-foreground">{{ t('editor.image.inset') }}</span>
        <Slider v-model="inset" :max="45" class="flex-1" :aria-label="t('editor.image.inset')" />
        <span class="w-10 text-right text-xs tabular-nums">{{ inset[0] }} %</span>
      </div>
      <ImageEditorDialog v-model:open="editorOpen" v-model:image="image" :shape="props.wide ? 'wide' : 'key'" :background="background" />
    </template>

    <IconPicker v-model:open="pickerOpen" @pick="emit('pickIcon', $event)" />
  </div>
</template>
