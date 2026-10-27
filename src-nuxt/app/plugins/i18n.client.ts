import { i18n, setLocale } from '~/lib/i18n'

/** The UI follows the engine's language: the Mac's, or the one chosen in the settings. */
export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.use(i18n)
  const { snapshot } = useEngine()
  watch(() => snapshot.value?.language, (language) => {
    if (language) {
      setLocale(language)
    }
  }, { immediate: true })
})
