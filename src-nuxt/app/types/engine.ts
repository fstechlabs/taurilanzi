import type { Action, Preferences } from '~/ipc/bindings'

export type * from '~/ipc/bindings'

export type NavigationAction = Extract<Action, { type: 'open_page' | 'back' | 'home' }>

/** Fields with a default are optional in the generated types, but the engine always sends them. */
type Complete<T> = T extends object ? { [K in keyof T]-?: Complete<T[K]> } : T

export type Settings = Complete<Preferences>
