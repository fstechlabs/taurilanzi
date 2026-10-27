import { commands } from '~/ipc/bindings'
import type { DeviceSettings, Settings } from '~/types/engine'

const SAVE_MS = 400

function copy<T>(value: T): T {
  return structuredClone(toRaw(value))
}

/** One group of settings, edited in place and saved a moment after the last change. Settings
 * stay out of undo, like the brightness. */
function editable<T extends object>(stored: () => T | undefined, save: (value: T) => Promise<unknown>) {
  const value = ref(null) as Ref<T | null>
  const failed = ref(false)
  let timer: ReturnType<typeof setTimeout> | undefined
  let pending = false

  watch(stored, (incoming) => {
    if (incoming && !pending) {
      value.value = copy(incoming)
    }
  }, { immediate: true, deep: true })

  async function flush() {
    clearTimeout(timer)
    if (!pending || !value.value) {
      return
    }
    pending = false
    try {
      await save(copy(value.value))
      failed.value = false
    }
    catch {
      failed.value = true
    }
  }

  watch(value, (next) => {
    const current = stored()
    if (!next || !current || JSON.stringify(next) === JSON.stringify(current)) {
      return
    }
    pending = true
    clearTimeout(timer)
    timer = setTimeout(flush, SAVE_MS)
  }, { deep: true })

  onBeforeUnmount(flush)

  return { value, failed }
}

/** The preferences and the dock's own settings, for the settings page. */
export function useSettings() {
  const { snapshot } = useEngine()
  const preferences = editable<Settings>(
    () => snapshot.value?.preferences as Settings | undefined,
    value => call(commands.setPreferences(value)),
  )
  const device = editable<DeviceSettings>(
    () => snapshot.value?.settings,
    value => call(commands.setDevice(value)),
  )
  const failed = computed(() => preferences.failed.value || device.failed.value)
  return { preferences: preferences.value, device: device.value, failed }
}
