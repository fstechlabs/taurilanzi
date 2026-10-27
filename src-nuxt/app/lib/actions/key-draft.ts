import type { InjectionKey, Ref } from 'vue'
import type { KeyDraft } from '~/composables/useKeyEditor'
import type { KeyRef } from '~/composables/useWorkspace'

/** The whole key the inspector edits, for action editors that change more than their action. */
export const KEY_DRAFT: InjectionKey<Ref<KeyDraft | null>> = Symbol('key draft')

/** The key the inspector edits: its page and its place on it. */
export const KEY_TARGET: InjectionKey<Ref<KeyRef | null>> = Symbol('key target')
