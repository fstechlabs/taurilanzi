import type { Scene } from '~/types/engine'
import { t } from '~/lib/i18n'

export const SCENES: { id: Scene, readonly name: string }[] = (['sunset', 'aurora', 'ocean', 'neon', 'mountains', 'graphite'] as const)
  .map(id => ({ id, get name() { return t(`editor.scenes.${id}`) } }))
