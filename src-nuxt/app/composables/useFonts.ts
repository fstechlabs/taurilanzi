import { commands } from '~/ipc/bindings'

const fonts = ref<string[]>([])
let loading: Promise<void> | null = null

/** Font families the renderer can draw text with, loaded once. */
export function useFonts() {
  if (!loading) {
    loading = call(commands.fonts())
      .then((list) => { fonts.value = list })
      .catch(() => { loading = null })
  }
  return { fonts: readonly(fonts) }
}
