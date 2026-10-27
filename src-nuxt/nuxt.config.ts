import tailwindcss from '@tailwindcss/vite'

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  modules: ['shadcn-nuxt'],
  shadcn: { prefix: '', componentDir: '@/components/ui' },
  components: [{ path: '~/components', pathPrefix: false, ignore: ['ui/**'] }],
  imports: { presets: [{ from: 'vue-i18n', imports: ['useI18n'] }] },
  compatibilityDate: '2025-07-15',
  ssr: false,
  devtools: { enabled: true },
  devServer: { port: 1420 },
  css: ['~/assets/css/tailwind.css'],
  vite: {
    plugins: [tailwindcss()],
    clearScreen: false,
    envPrefix: ['VITE_', 'TAURI_'],
    server: { strictPort: true },
  },
})
