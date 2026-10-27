// Lets the WebdriverIO service drive the app. Only builds for the end-to-end tests set
// VITE_E2E, so everywhere else this import is removed at build time.
export default defineNuxtPlugin(async () => {
  if (import.meta.env.VITE_E2E) {
    await import('@wdio/tauri-plugin')
  }
})
