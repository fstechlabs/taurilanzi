import type { Appearance, Settings } from '~/types/engine'
// Relative, not `~`: the unit tests import this file without Nuxt's alias.
import { LOCALES, i18n } from './i18n'

export type NewKeyStyle = Settings['new_keys']

/** A key just created, in the style chosen in the settings, with `content` as its text. */
export function newKeyAppearance(style: NewKeyStyle, content: string): Appearance {
  return {
    background: { type: 'solid', color: { ...style.background } },
    layers: [{
      type: 'text',
      content,
      font_family: style.font_family,
      font_weight: style.font_weight,
      size: style.text_size,
      color: { ...style.text_color },
      position: style.text_position,
      outline: null,
    }],
  }
}

/** Whether `text` is still the text a key is created with (`editor.newKey.text`), in whichever
 * language it was created. */
export function isDefaultKeyText(text: string | null | undefined): boolean {
  const global = i18n.global as unknown as { t: (key: string, named: object, options: { locale: string }) => string }
  return LOCALES.some(locale => text === global.t('editor.newKey.text', {}, { locale }))
}
