/** A key of a page: what the inspector edits. */
export interface KeyRef { page: string, index: number }

const selection = ref<KeyRef | null>(null)
/** The line picked on the canvas, if the selection came from clicking one. */
const selectedLink = ref<string | null>(null)
/** A page the canvas should bring into view; the counter repeats requests for the same page. */
const focusRequest = ref<{ page: string, count: number } | null>(null)

/** What the user is looking at and editing, shared by the canvas, the sidebar and the inspector. */
export function useWorkspace() {
  const { snapshot } = useEngine()

  const pages = computed(() => snapshot.value?.pages ?? [])
  const current = computed(() => snapshot.value?.path.at(-1) ?? null)

  function select(page: string, index: number) {
    selection.value = { page, index }
    selectedLink.value = null
  }

  /** Picking a line also selects its key, whose actions the inspector shows. */
  function selectLink(page: string, index: number) {
    selection.value = { page, index }
    selectedLink.value = `${page}:${index}`
  }

  function clear() {
    selection.value = null
    selectedLink.value = null
  }

  function focus(page: string) {
    focusRequest.value = { page, count: (focusRequest.value?.count ?? 0) + 1 }
  }

  watch(pages, (list) => {
    if (selection.value && !list.some(p => p.id === selection.value!.page)) {
      selection.value = null
    }
  })

  return { selection, selectedLink, focusRequest, pages, current, select, selectLink, clear, focus }
}
