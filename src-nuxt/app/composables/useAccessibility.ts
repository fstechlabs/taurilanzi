import { commands } from '~/ipc/bindings'

const granted = ref(true)
let watching = false

async function check() {
  granted.value = await commands.accessibilityGranted()
}

/** Whether the system lets the app press keys and type, checked again whenever the window is
 * back. */
export function useAccessibility() {
  if (!watching && import.meta.client) {
    watching = true
    check()
    window.addEventListener('focus', check)
  }

  /** On Linux it returns once the password was given, and the permission is there. */
  async function request() {
    await commands.requestAccessibility()
    await check()
  }

  return { granted: readonly(granted), check, request }
}
