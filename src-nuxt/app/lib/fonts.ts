import { t } from '~/lib/i18n'

export const FONT_WEIGHTS: { value: number, readonly label: string }[] = ([
  [400, 'regular'],
  [500, 'medium'],
  [600, 'semibold'],
  [700, 'bold'],
  [900, 'black'],
] as const).map(([value, key]) => ({ value, get label() { return t(`editor.fontWeights.${key}`) } }))
