import { isTauri } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

/** Opens a link in the default browser instead of inside the Panel window. */
export async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url)
  }
  else {
    window.open(url, '_blank', 'noopener')
  }
}
