import { linkColors } from '~/lib/link-colors'

/** Colour of every line on the canvas, by line id (`page:key`), shared by lines and key handles. */
export function useLinkColors() {
  const { pages } = useWorkspace()
  return computed(() => linkColors(pages.value.flatMap(page =>
    Object.entries(page.keys)
      .filter(([, key]) => key.actions.some(a => a.type === 'open_page'))
      .map(([index]) => `${page.id}:${index}`),
  )))
}
