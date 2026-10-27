<script setup lang="ts">
import { Braces } from '@lucide/vue'
import type { StateDraft, TextDraft } from '~/composables/useKeyEditor'
import { FONT_WEIGHTS } from '~/lib/fonts'

const draft = defineModel<StateDraft>({ required: true })
const { fonts } = useFonts()
const { t } = useI18n()

const enabled = computed({
  get: () => draft.value.text !== null,
  set: (on) => {
    draft.value.text = on
      ? { content: t('editor.text.default'), fontFamily: 'Inter', fontWeight: 700, size: 30, color: '#ffffff', position: 'bottom' }
      : null
  },
})
const size = computed({
  get: () => [draft.value.text?.size ?? 30],
  set: ([value]) => {
    if (draft.value.text) {
      draft.value.text.size = value ?? 30
    }
  },
})
/** Filled in with the page the key is on when it is drawn. */
const VARIABLES = [
  { token: '{page}', label: 'editor.text.variables.page' },
  { token: '{page_number}', label: 'editor.text.variables.pageNumber' },
  { token: '{page_count}', label: 'editor.text.variables.pageCount' },
]

const textarea = ref<HTMLTextAreaElement | null>(null)

/** Puts a variable where the cursor is, or at the end. */
function insert(token: string) {
  const text = draft.value.text
  if (!text) {
    return
  }
  const el = textarea.value
  const at = el ? el.selectionStart : text.content.length
  const end = el ? el.selectionEnd : at
  text.content = text.content.slice(0, at) + token + text.content.slice(end)
  nextTick(() => {
    el?.focus()
    el?.setSelectionRange(at + token.length, at + token.length)
  })
}

const weight = computed({
  get: () => String(draft.value.text?.fontWeight ?? 700),
  set: (value) => {
    if (draft.value.text) {
      draft.value.text.fontWeight = Number(value)
    }
  },
})
</script>

<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center justify-between">
      <Label class="text-xs text-muted-foreground">{{ t('editor.text.title') }}</Label>
      <Switch v-model="enabled" :aria-label="t('editor.text.show')" />
    </div>
    <template v-if="draft.text">
      <textarea
        ref="textarea"
        v-model="draft.text.content"
        rows="2"
        class="w-full resize-none rounded-md border bg-transparent px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :aria-label="t('editor.text.content')"
      />
      <DropdownMenu>
        <DropdownMenuTrigger as-child>
          <Button variant="ghost" size="sm" class="h-7 justify-start px-2 text-xs text-muted-foreground">
            <Braces /> {{ t('editor.text.insert') }}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" class="w-56">
          <DropdownMenuItem v-for="v in VARIABLES" :key="v.token" @select="insert(v.token)">
            {{ t(v.label) }} <span class="ml-auto font-mono text-[11px] text-muted-foreground">{{ v.token }}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <div class="grid grid-cols-2 gap-2">
        <Select v-model="draft.text.fontFamily">
          <SelectTrigger class="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent class="max-h-72">
            <SelectItem v-for="font in fonts" :key="font" :value="font">
              {{ font }}
            </SelectItem>
          </SelectContent>
        </Select>
        <Select v-model="weight">
          <SelectTrigger class="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="w in FONT_WEIGHTS" :key="w.value" :value="String(w.value)">
              {{ w.label }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div class="flex items-center gap-3">
        <span class="w-14 text-xs text-muted-foreground">{{ t('editor.text.size') }}</span>
        <Slider v-model="size" :min="10" :max="80" class="flex-1" />
        <span class="w-10 text-right text-xs tabular-nums">{{ draft.text.size }}</span>
      </div>
      <div class="flex items-center gap-2">
        <ColorPicker v-model="draft.text.color" :label="t('editor.text.color')" compact />
        <ToggleGroup :model-value="draft.text.position" type="single" @update:model-value="v => v && draft.text && (draft.text.position = v as TextDraft['position'])" variant="outline" size="sm" class="flex-1">
          <ToggleGroupItem value="top" class="flex-1">
            {{ t('editor.text.top') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="middle" class="flex-1">
            {{ t('editor.text.middle') }}
          </ToggleGroupItem>
          <ToggleGroupItem value="bottom" class="flex-1">
            {{ t('editor.text.bottom') }}
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
    </template>
  </div>
</template>
