import type { InjectionKey } from 'vue'
import type { KeySetup } from '~/lib/actions/catalog'

/** What the inspector lets the action catalog do to the whole key, not just one list. */
export interface KeySetupHandler {
  /** Why it cannot be done to this key, if it cannot. */
  unavailable: (setup: KeySetup) => string | null
  apply: (setup: KeySetup) => void
}

export const KEY_SETUP: InjectionKey<KeySetupHandler> = Symbol('key setup')
