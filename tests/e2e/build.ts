import { join } from 'node:path'

const app = join(import.meta.dirname, '..', '..')
/** Apart from `tauri dev`'s, so the two builds never overwrite each other. */
export const TARGET_DIR = join(app, 'src-tauri', 'target', 'e2e')
export const APP_BINARY = join(TARGET_DIR, 'debug', 'taurilanzi')
