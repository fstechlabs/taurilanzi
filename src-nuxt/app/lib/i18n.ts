import type { WritableComputedRef } from 'vue'
import { createI18n } from 'vue-i18n'
import en from '../locales/en.json'
import es from '../locales/es.json'

export type Locale = 'es' | 'en'
export const LOCALES: Locale[] = ['es', 'en']

/** Spanish for any Spanish locale, English for everything else, as the engine decides. */
export function localeOf(tag: string | undefined): Locale {
  return tag?.toLowerCase().startsWith('es') ? 'es' : 'en'
}

export const i18n = createI18n({
  legacy: false,
  locale: localeOf(typeof navigator === 'undefined' ? undefined : navigator.language),
  fallbackLocale: 'en',
  messages: { es, en },
  missingWarn: import.meta.env?.DEV ?? false,
  fallbackWarn: false,
})

/** For code outside components: always the language shown now. */
export function t(key: string, values?: Record<string, unknown> | number): string {
  const global = i18n.global as unknown as { t: (key: string, values: Record<string, unknown> | number) => string }
  return global.t(key, values ?? {})
}

export function setLocale(locale: Locale) {
  ;(i18n.global.locale as unknown as WritableComputedRef<string>).value = locale
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale
  }
}
