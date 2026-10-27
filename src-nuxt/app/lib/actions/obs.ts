import type { Action, ObsCommand, ObsInfo } from '~/types/engine'
import type { KeyDraft, StateDraft } from '~/composables/useKeyEditor'
// Relative, not `~`: the unit tests import this file and bun does not know that alias.
import { t } from '../i18n'

type Kind = ObsCommand['kind']

const SWITCHES: Kind[] = ['record', 'record_pause', 'stream', 'virtual_cam', 'replay_buffer', 'studio_mode', 'source_visibility', 'filter', 'mute']

function camel(text: string) {
  return text.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
}

/** How each way of a command that turns something on or off is called, if it is one. */
export function switchLabels(kind: Kind): { on: string, off: string } | null {
  if (!SWITCHES.includes(kind)) {
    return null
  }
  return { on: t(`actions.obs.switch.${camel(kind)}.on`), off: t(`actions.obs.switch.${camel(kind)}.off`) }
}

/** A state of OBS a key of two states can follow, and the color of its second look. */
export interface FollowedState {
  /** Ends "muestra el estado 2 …". */
  while: string
  color: string
}

const RED = '#dc2626'
const GREEN = '#16a34a'

/** The color of the second look of a key that follows each state. */
const FOLLOWED: Partial<Record<Kind, string>> = {
  record: RED,
  record_pause: '#d97706',
  stream: RED,
  virtual_cam: GREEN,
  replay_buffer: GREEN,
  studio_mode: '#2563eb',
  source_visibility: GREEN,
  filter: GREEN,
  mute: RED,
}

/** What a key of two states with this command shows, if OBS has such a state. */
export function followedState(command: ObsCommand): FollowedState | null {
  if (command.kind === 'scene') {
    return command.preview
      ? { while: t('actions.obs.while.scenePreview'), color: GREEN }
      : { while: t('actions.obs.while.sceneProgram'), color: RED }
  }
  const color = FOLLOWED[command.kind]
  return color ? { while: t(`actions.obs.while.${camel(command.kind)}`), color } : null
}

/** The command a key of two states follows, as the engine picks it: the first with a state. */
export function followedCommand(first: Action[], second: Action[]): ObsCommand | null {
  for (const action of [...first, ...second]) {
    if (action.type === 'obs' && followedState(action.command)) {
      return action.command
    }
  }
  return null
}

export function sameCommand(a: ObsCommand, b: ObsCommand) {
  return JSON.stringify(a) === JSON.stringify(b)
}

/** Gives the key a second state that follows the command: a copy of the first look in the
 * color of the state, doing the same. */
export function followInKey(draft: KeyDraft, command: ObsCommand, server: string) {
  const state = followedState(command)
  if (!state || draft.second) {
    return
  }
  const { background, from, to, angle, image, text, untouched, widget } = draft
  const second = JSON.parse(JSON.stringify({ background, from, to, angle, image, text, untouched, widget })) as Omit<StateDraft, 'actions'>
  draft.second = {
    ...second,
    background: 'solid',
    from: state.color,
    actions: [{ type: 'obs', server, command: JSON.parse(JSON.stringify(command)) as ObsCommand }],
  }
}

/** What is missing for an OBS command to be saved, the same checks the engine makes. */
export function obsError(command: ObsCommand): string | null {
  const blank = (name: string) => !name.trim()
  switch (command.kind) {
    case 'set_transition':
      if (blank(command.transition)) {
        return t('actions.obs.errors.transition')
      }
      if (command.duration_ms != null && (command.duration_ms < 50 || command.duration_ms > 20000)) {
        return t('actions.obs.errors.duration')
      }
      return null
    case 'scene_collection':
      return blank(command.collection) ? t('actions.obs.errors.collection') : null
    case 'profile':
      return blank(command.profile) ? t('actions.obs.errors.profile') : null
    case 'scene':
      return blank(command.scene) ? t('actions.obs.errors.scene') : null
    case 'source_visibility':
      return blank(command.scene) || blank(command.source) ? t('actions.obs.errors.sceneAndSource') : null
    case 'filter':
      return blank(command.source) || blank(command.filter) ? t('actions.obs.errors.sourceAndFilter') : null
    case 'media':
    case 'mute':
      return blank(command.input) ? t('actions.obs.errors.source') : null
    case 'volume':
      if (blank(command.input)) {
        return t('actions.obs.errors.source')
      }
      if (command.change.type === 'step' && (command.change.db === 0 || Math.abs(command.change.db) > 30)) {
        return t('actions.obs.errors.step')
      }
      if (command.change.type === 'set' && command.change.percent > 100) {
        return t('actions.obs.errors.volume')
      }
      return null
    default:
      return null
  }
}

/** The connection to OBS in a few words. */
export function obsStatusText(info: ObsInfo): string {
  const status = info.status
  switch (status.state) {
    case 'off':
      return t('actions.obs.status.off')
    case 'connecting':
      return t('actions.obs.status.connecting')
    case 'connected':
      return t('actions.obs.status.connected', { version: status.obs_version })
    case 'failed':
      switch (status.reason) {
        case 'unreachable':
          return t('actions.obs.status.unreachable')
        case 'password_required':
          return t('actions.obs.status.passwordRequired')
        case 'wrong_password':
          return t('actions.obs.status.wrongPassword')
        case 'incompatible':
          return t('actions.obs.status.incompatible')
        default:
          return t('actions.obs.status.failed')
      }
  }
}
