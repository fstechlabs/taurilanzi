<script setup lang="ts">
import { Maximize, Plus, Redo2, Undo2 } from '@lucide/vue'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import { MarkerType, Panel, VueFlow, useVueFlow } from '@vue-flow/core'
import type { Connection, Edge, Node, NodeDragEvent } from '@vue-flow/core'
import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/controls/dist/style.css'
import { REDO_SHORTCUT, UNDO_SHORTCUT } from '~/lib/actions/keys'
import type { PageNodeData } from '~/types/canvas'

const FLOW = 'mesa'
const CARD = { width: 320, height: 240 }
const GAP = 80

const { snapshot, refresh } = useEngine()
const { pages, current, focusRequest, selectedLink, clear } = useWorkspace()
const actions = usePages()
const colors = useLinkColors()
const { fitView, screenToFlowCoordinate } = useVueFlow(FLOW)
const contextMenu = useContextMenu()
const history = useHistory()
const { t } = useI18n()

const error = ref<string | null>(null)
const renaming = ref<{ id: string, name: string } | null>(null)

const nodes = computed<Node<PageNodeData>[]>(() => pages.value.map(page => ({
  id: page.id,
  type: 'page',
  position: { x: page.position.x, y: page.position.y },
  dragHandle: '.page-drag',
  data: { page, home: page.id === snapshot.value?.home_page, onDock: page.id === current.value },
})))

const edges = computed<Edge[]>(() => pages.value.flatMap(page =>
  Object.entries(page.keys).flatMap(([index, key]) => key.actions
    .filter(a => a.type === 'open_page')
    .map((a) => {
      const id = `${page.id}:${index}`
      const color = colors.value.get(id) ?? '#38bdf8'
      const waypoints = a.type === 'open_page' ? (a.waypoints ?? []) : []
      return {
        id,
        type: 'link',
        source: page.id,
        sourceHandle: `key-${index}`,
        target: (a as { page: string }).page,
        targetHandle: 'in',
        data: { color, waypoints, page: page.id, index: Number(index) },
        // A fixed size on the canvas, so the arrow does not grow with a highlighted line.
        markerEnd: { type: MarkerType.ArrowClosed, color, width: 36, height: 36, markerUnits: 'userSpaceOnUse' },
        // Over every page: each line has its own colour, so they stay easy to follow.
        zIndex: 2000,
        selectable: false,
        focusable: false,
      }
    })),
))

async function attempt(work: () => Promise<unknown>, message: string) {
  error.value = null
  try {
    await work()
  }
  catch {
    error.value = message
  }
}

function keyIndex(handle: string | null | undefined) {
  return Number(handle?.replace('key-', ''))
}

function connect(connection: Connection) {
  const index = keyIndex(connection.sourceHandle)
  if (connection.source === connection.target || !Number.isInteger(index)) {
    return
  }
  attempt(() => actions.link(connection.source, index, connection.target), t('editor.canvas.errors.link'))
}

/** A line and the key it leaves from go together. */
function removeLine(edgeId: string) {
  const [page, index] = edgeId.split(':')
  attempt(() => actions.removeKey(page!, Number(index)), t('editor.canvas.errors.removeKey'))
}

function paneMenu(event: MouseEvent) {
  const at = screenToFlowCoordinate({ x: event.clientX, y: event.clientY })
  contextMenu.open(event, [{
    label: t('editor.canvas.newPageHere'),
    icon: Plus,
    run: () => attempt(() => actions.create({ x: Math.round(at.x), y: Math.round(at.y) }), t('editor.canvas.errors.create')),
  }])
}

function moved(event: NodeDragEvent) {
  for (const node of event.nodes) {
    const position = { x: Math.round(node.position.x), y: Math.round(node.position.y) }
    attempt(() => actions.update(node.id, { position }), t('editor.canvas.errors.move'))
  }
}

/** Fitting has to wait until the new page has been measured. */
let fitAfterAdd = false

/** New pages go to the right of the rightmost one, so they never cover another. */
async function addPage() {
  const list = pages.value
  const right = list.reduce((max, p) => (p.position.x > max.position.x ? p : max), list[0]!)
  const position = right
    ? { x: right.position.x + CARD.width + GAP, y: right.position.y }
    : { x: 0, y: 0 }
  fitAfterAdd = true
  await attempt(() => actions.create(position).then(refresh), t('editor.canvas.errors.create'))
}

function initialized() {
  if (fitAfterAdd) {
    fitAfterAdd = false
    fitView({ padding: 0.2, duration: 300 })
  }
}

function startRename(id: string) {
  renaming.value = { id, name: actions.page(id)?.name ?? '' }
}

async function rename() {
  const target = renaming.value
  if (!target || !target.name.trim()) {
    return
  }
  renaming.value = null
  await attempt(() => actions.update(target.id, { name: target.name.trim() }), t('editor.canvas.errors.rename'))
}

async function remove(id: string) {
  await attempt(() => actions.remove(id), t('editor.canvas.errors.remove'))
}

function keydown(event: KeyboardEvent) {
  const typing = event.target instanceof HTMLElement && event.target.closest('input, textarea, [contenteditable]')
  if (selectedLink.value && !typing && (event.key === 'Delete' || event.key === 'Backspace')) {
    removeLine(selectedLink.value)
    clear()
  }
}

onMounted(() => window.addEventListener('keydown', keydown))
onBeforeUnmount(() => window.removeEventListener('keydown', keydown))

function focusPage(page: string) {
  fitView({ nodes: [page], padding: 0.3, maxZoom: 1.2, duration: 300 })
}

watch(focusRequest, (request) => {
  if (request) {
    focusPage(request.page)
  }
})

</script>

<template>
  <div class="mesa relative size-full">
    <VueFlow
      :id="FLOW"
      :nodes="nodes"
      :edges="edges"
      :min-zoom="0.2"
      :max-zoom="1.5"
      :delete-key-code="null"
      :connection-radius="30"
      :is-valid-connection="c => c.source !== c.target"
      :default-viewport="{ zoom: 0.8 }"
      fit-view-on-init
      class="size-full"
      @connect="connect"
      @node-drag-stop="moved"
      @nodes-initialized="initialized"
      @pane-click="clear"
      @pane-context-menu="paneMenu"
      @node-double-click="({ node }) => focusPage(node.id)"
    >
      <template #node-page="props">
        <PageNode
          v-bind="props"
          @show="attempt(() => actions.show(props.id), t('editor.canvas.errors.show'))"
          @rename="startRename(props.id)"
          @duplicate="attempt(() => actions.duplicate(props.id), t('editor.canvas.errors.duplicate'))"
          @home="attempt(() => actions.setHome(props.id), t('editor.canvas.errors.home'))"
          @remove="remove(props.id)"
        />
      </template>
      <template #edge-link="props">
        <LinkEdge v-bind="props" @remove="removeLine(props.id)" />
      </template>

      <Background :gap="24" pattern-color="#2a2a2a" />
      <Controls :show-interactive="false" position="bottom-left" />

      <Panel position="top-left" class="flex items-center gap-2">
        <Button size="sm" @click="addPage">
          <Plus /> {{ t('editor.canvas.newPage') }}
        </Button>
        <Button variant="outline" size="sm" @click="fitView({ padding: 0.2, duration: 300 })">
          <Maximize /> {{ t('editor.canvas.fitAll') }}
        </Button>
        <div class="flex items-center">
          <Button variant="outline" size="icon-sm" class="rounded-r-none" :disabled="!history.canUndo.value" :aria-label="t('editor.canvas.undo')" :title="t('editor.canvas.undoTitle', { shortcut: UNDO_SHORTCUT })" @click="history.undo()">
            <Undo2 />
          </Button>
          <Button variant="outline" size="icon-sm" class="-ml-px rounded-l-none" :disabled="!history.canRedo.value" :aria-label="t('editor.canvas.redo')" :title="t('editor.canvas.redoTitle', { shortcut: REDO_SHORTCUT })" @click="history.redo()">
            <Redo2 />
          </Button>
        </div>
        <span v-if="error" class="rounded-md bg-destructive/15 px-2 py-1 text-xs text-destructive">{{ error }}</span>
      </Panel>
    </VueFlow>

    <Dialog :open="Boolean(renaming)" @update:open="v => v || (renaming = null)">
      <DialogContent class="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ t('editor.canvas.rename') }}</DialogTitle>
        </DialogHeader>
        <form v-if="renaming" class="flex flex-col gap-4" @submit.prevent="rename">
          <Input v-model="renaming.name" :aria-label="t('editor.canvas.pageName')" autofocus maxlength="60" />
          <DialogFooter>
            <Button type="button" variant="outline" @click="renaming = null">
              {{ t('editor.canvas.cancel') }}
            </Button>
            <Button type="submit" :disabled="!renaming.name.trim()">
              {{ t('editor.canvas.save') }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  </div>
</template>

<style>
/* Dragging lines and keys must not select the page's text. */
.mesa {
  -webkit-user-select: none;
  user-select: none;
}
.mesa .vue-flow__node-page {
  padding: 0;
  border: 0;
  background: transparent;
  box-shadow: none;
}
.mesa .vue-flow__handle.key-handle {
  width: 9px;
  height: 9px;
  right: -5px;
  border: 1.5px solid #0a0a0a;
  background: #525252;
  opacity: 0;
  transition: opacity 120ms;
}
.mesa .vue-flow__node:hover .key-handle,
.mesa .vue-flow__handle.key-handle.linked {
  opacity: 1;
}
.mesa .vue-flow__handle.key-handle.linked {
  background: #38bdf8;
}
.mesa .vue-flow__handle.page-target {
  inset: 0;
  border-radius: 1rem;
  width: 100%;
  height: 100%;
  transform: none;
  border: 0;
  border-radius: 0.75rem;
  background: transparent;
  pointer-events: none;
}
.mesa .vue-flow__handle.page-target.active {
  pointer-events: all;
  z-index: 10;
  background: rgb(56 189 248 / 0.08);
}
.mesa .vue-flow__controls {
  box-shadow: none;
}
.mesa .vue-flow__controls-button {
  border-color: #262626;
  background: #171717;
  fill: #d4d4d4;
}
.mesa .vue-flow__controls-button:hover {
  background: #262626;
}
/* Above the lines, which are above the pages, so the remove button can be clicked. */
.mesa .vue-flow__edge-labels {
  position: relative;
  z-index: 2001;
}
.mesa .link-path {
  fill: none;
  stroke-width: 2;
  pointer-events: none;
  transition: stroke-width 120ms;
}
.mesa .link-path.active {
  stroke-width: 4;
}
/* Thin enough to leave the keys underneath usable, wide enough to pick the line. */
.mesa .link-hit {
  fill: none;
  stroke: transparent;
  pointer-events: stroke;
  cursor: grab;
}
.mesa .link-hit.pulling {
  cursor: grabbing;
}
.mesa .vue-flow__edge:hover .link-path {
  stroke-width: 3;
}
.mesa .vue-flow__connection-path {
  stroke: #38bdf8;
  stroke-width: 2;
}
</style>
