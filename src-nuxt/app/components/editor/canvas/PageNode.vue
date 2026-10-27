<script setup lang="ts">
import { Copy, Ellipsis, House, MonitorPlay, Pencil, Trash2 } from '@lucide/vue'
import { Handle, Position, useVueFlow } from '@vue-flow/core'
import type { NodeProps } from '@vue-flow/core'
import type { PageNodeData } from '~/types/canvas'

const props = defineProps<NodeProps<PageNodeData>>()
const emit = defineEmits<{ rename: [], duplicate: [], remove: [], home: [], show: [] }>()

const { selection, select } = useWorkspace()
const { connectionStartHandle } = useVueFlow()
const colors = useLinkColors()
const contextMenu = useContextMenu()
const { t } = useI18n()

/** The same choices as the ⋯ menu, on a right click on the page name. */
function menu(event: MouseEvent) {
  contextMenu.open(event, [
    { label: t('editor.page.show'), icon: MonitorPlay, disabled: props.data.onDock, run: () => emit('show') },
    { label: t('editor.page.rename'), icon: Pencil, separated: true, run: () => emit('rename') },
    { label: t('editor.page.duplicate'), icon: Copy, run: () => emit('duplicate') },
    { label: t('editor.page.home'), icon: House, disabled: props.data.home, run: () => emit('home') },
    { label: t('editor.page.remove'), icon: Trash2, destructive: true, separated: true, disabled: props.data.home, run: () => emit('remove') },
  ])
}

const page = computed(() => props.data.page)
const selectedKey = computed(() => (selection.value?.page === page.value.id ? selection.value.index : null))
/** The whole dock takes the line while one is being dragged from another page. */
const dropTarget = computed(() => {
  const start = connectionStartHandle.value
  return Boolean(start && start.nodeId !== props.id)
})

function linked(index: number) {
  return page.value.keys[index]?.actions.some(a => a.type === 'open_page') ?? false
}
</script>

<template>
  <div class="w-80">
    <DockGrid
      :page="page.id"
      :selected="selectedKey"
      compact
      class="transition"
      :class="[
        props.selected ? 'ring-2 ring-sky-500' : props.data.onDock ? 'ring-2 ring-emerald-600/80' : '',
        dropTarget ? 'ring-2 ring-sky-400' : '',
      ]"
      :data-page="page.name"
      @select="select(page.id, $event)"
    >
      <template #header>
        <Handle
          id="in"
          type="target"
          :position="Position.Left"
          :connectable-start="false"
          class="page-target"
          :class="{ active: dropTarget }"
        />
        <div class="page-drag mb-[2.5%] flex cursor-grab items-center gap-1.5 active:cursor-grabbing" @contextmenu="menu">
          <House v-if="props.data.home" class="size-3 shrink-0 text-amber-400" :aria-label="t('editor.page.homePage')" />
          <span class="min-w-0 flex-1 truncate text-xs font-medium text-neutral-200">{{ page.name }}</span>
          <span v-if="props.data.onDock" class="flex shrink-0 items-center gap-1 text-[9px] text-emerald-400">
            <span class="size-1.5 rounded-full bg-emerald-400" /> {{ t('editor.page.onDock') }}
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger as-child>
              <Button variant="ghost" size="icon-sm" class="nodrag size-5 text-neutral-400" :aria-label="t('editor.page.options', { name: page.name })">
                <Ellipsis />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" class="w-52">
              <DropdownMenuItem :disabled="props.data.onDock" @select="emit('show')">
                <MonitorPlay /> {{ t('editor.page.show') }}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem @select="emit('rename')">
                <Pencil /> {{ t('editor.page.rename') }}
              </DropdownMenuItem>
              <DropdownMenuItem @select="emit('duplicate')">
                <Copy /> {{ t('editor.page.duplicate') }}
              </DropdownMenuItem>
              <DropdownMenuItem :disabled="props.data.home" @select="emit('home')">
                <House /> {{ t('editor.page.home') }}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem :disabled="props.data.home" variant="destructive" @select="emit('remove')">
                <Trash2 /> {{ t('editor.page.remove') }}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </template>
      <template #key="{ index }">
        <Handle
          :id="`key-${index}`"
          type="source"
          :position="Position.Right"
          class="key-handle"
          :class="{ linked: linked(index) }"
          :style="linked(index) ? { background: colors.get(`${page.id}:${index}`) } : undefined"
          :aria-label="t('editor.page.linkKey', { number: index + 1 })"
        />
      </template>
    </DockGrid>
  </div>
</template>
