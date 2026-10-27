import { describe, expect, test } from 'bun:test'
import type { Action, ObsCommand } from '../../src-nuxt/app/ipc/bindings'
import type { KeyDraft, StateDraft } from '../../src-nuxt/app/composables/useKeyEditor'
import { followInKey, followedCommand, obsError, sameCommand } from '../../src-nuxt/app/lib/actions/obs'

describe('obsError', () => {
  const cases: [ObsCommand, ObsCommand, string][] = [
    [{ kind: 'set_transition', transition: ' ' }, { kind: 'set_transition', transition: 'Fundido', duration_ms: 300 }, 'Elige la transición'],
    [{ kind: 'scene_collection', collection: '' }, { kind: 'scene_collection', collection: 'Directo' }, 'Elige la colección de escenas'],
    [{ kind: 'profile', profile: '' }, { kind: 'profile', profile: 'Twitch' }, 'Elige el perfil de OBS'],
    [{ kind: 'scene', scene: '  ' }, { kind: 'scene', scene: 'Cámara' }, 'Elige la escena'],
    [{ kind: 'source_visibility', scene: 'Cámara', source: '', mode: 'toggle' }, { kind: 'source_visibility', scene: 'Cámara', source: 'Logo', mode: 'on' }, 'Elige la escena y la fuente'],
    [{ kind: 'filter', source: '', filter: 'Desenfoque', mode: 'off' }, { kind: 'filter', source: 'Cámara', filter: 'Desenfoque', mode: 'off' }, 'Elige la fuente y el filtro'],
    [{ kind: 'media', input: '', control: 'play' }, { kind: 'media', input: 'Intro', control: 'play' }, 'Elige la fuente'],
    [{ kind: 'mute', input: '', mode: 'toggle' }, { kind: 'mute', input: 'Mic', mode: 'toggle' }, 'Elige la fuente'],
    [{ kind: 'volume', input: '', change: { type: 'step', db: 3 } }, { kind: 'volume', input: 'Mic', change: { type: 'step', db: -3 } }, 'Elige la fuente'],
  ]

  test('names that must be chosen', () => {
    for (const [missing, complete, message] of cases) {
      expect(obsError(missing)).toBe(message)
      expect(obsError(complete)).toBeNull()
    }
  })

  test('transition length stays between 50 ms and 20 s', () => {
    const message = 'Una transición dura entre 50 ms y 20 s'
    expect(obsError({ kind: 'set_transition', transition: 'Corte', duration_ms: 49 })).toBe(message)
    expect(obsError({ kind: 'set_transition', transition: 'Corte', duration_ms: 20001 })).toBe(message)
    expect(obsError({ kind: 'set_transition', transition: 'Corte', duration_ms: 50 })).toBeNull()
    expect(obsError({ kind: 'set_transition', transition: 'Corte', duration_ms: 20000 })).toBeNull()
    expect(obsError({ kind: 'set_transition', transition: 'Corte', duration_ms: null })).toBeNull()
  })

  test('volume steps and levels', () => {
    const step = (db: number): ObsCommand => ({ kind: 'volume', input: 'Mic', change: { type: 'step', db } })
    const set = (percent: number): ObsCommand => ({ kind: 'volume', input: 'Mic', change: { type: 'set', percent } })
    expect(obsError(step(0))).toBe('Un paso de volumen va de 1 a 30 dB')
    expect(obsError(step(31))).toBe('Un paso de volumen va de 1 a 30 dB')
    expect(obsError(step(-31))).toBe('Un paso de volumen va de 1 a 30 dB')
    expect(obsError(step(30))).toBeNull()
    expect(obsError(step(-30))).toBeNull()
    expect(obsError(set(101))).toBe('El volumen llega hasta 100')
    expect(obsError(set(100))).toBeNull()
    expect(obsError(set(0))).toBeNull()
  })

  test('commands without names are always fine', () => {
    const free: ObsCommand[] = [
      { kind: 'record', mode: 'toggle' },
      { kind: 'record_pause', mode: 'on' },
      { kind: 'stream', mode: 'off' },
      { kind: 'virtual_cam', mode: 'toggle' },
      { kind: 'replay_buffer', mode: 'toggle' },
      { kind: 'save_replay' },
      { kind: 'studio_mode', mode: 'toggle' },
      { kind: 'transition' },
      { kind: 'screenshot' },
      { kind: 'chapter' },
    ]
    for (const command of free) {
      expect(obsError(command)).toBeNull()
    }
  })
})

const obs = (command: ObsCommand): Action => ({ type: 'obs', command })

describe('followedCommand', () => {
  test('the first command with a state, first list before the second', () => {
    const record: ObsCommand = { kind: 'record', mode: 'toggle' }
    const mute: ObsCommand = { kind: 'mute', input: 'Mic', mode: 'toggle' }
    const first: Action[] = [{ type: 'delay', ms: 10 }, obs({ kind: 'save_replay' }), obs(record), obs(mute)]
    expect(followedCommand(first, [obs(mute)])).toEqual(record)
    expect(followedCommand([obs({ kind: 'transition' })], [obs(mute), obs(record)])).toEqual(mute)
  })

  test('scenes have a state, on the program or the preview', () => {
    const scene: ObsCommand = { kind: 'scene', scene: 'Cámara', preview: true }
    expect(followedCommand([], [obs(scene)])).toEqual(scene)
  })

  test('nothing to follow', () => {
    expect(followedCommand([], [])).toBeNull()
    expect(followedCommand([obs({ kind: 'chapter' }), { type: 'home' }], [obs({ kind: 'screenshot' })])).toBeNull()
  })
})

test('sameCommand compares the whole command', () => {
  expect(sameCommand({ kind: 'record', mode: 'on' }, { kind: 'record', mode: 'on' })).toBe(true)
  expect(sameCommand({ kind: 'record', mode: 'on' }, { kind: 'record', mode: 'off' })).toBe(false)
  expect(sameCommand({ kind: 'scene', scene: 'A' }, { kind: 'scene', scene: 'B' })).toBe(false)
})

function draft(): KeyDraft {
  return {
    background: 'gradient',
    from: '#111111',
    to: '#222222',
    angle: 45,
    image: { asset: 'img', source: 'img', crop: null, fit: 'contain', inset: 0.2, tint: '#ffffff', icon: 'lucide:mic' },
    text: { content: 'Grabar', fontFamily: 'Inter', fontWeight: 600, size: 20, color: '#ffffff', position: 'bottom' },
    untouched: [{ type: 'border', color: { r: 1, g: 1, b: 1 }, width: 2, radius: 8 }],
    actions: [obs({ kind: 'record', mode: 'toggle' })],
    widget: { kind: 'network', graph: true, minutes: 10, style: { color: { r: 255, g: 255, b: 255 }, accent: { r: 0, g: 0, b: 255 }, font_family: 'Inter' } },
    doublePress: [],
    longPress: [],
    second: null,
  }
}

describe('followInKey', () => {
  test('adds a second state in the color of the state, copying the first look', () => {
    const key = draft()
    const command: ObsCommand = { kind: 'record', mode: 'toggle' }
    followInKey(key, command)
    const second = key.second!
    expect(second.background).toBe('solid')
    expect(second.from).toBe('#dc2626')
    expect(second.to).toBe(key.to)
    expect(second.angle).toBe(key.angle)
    expect(second.image).toEqual(key.image)
    expect(second.text).toEqual(key.text)
    expect(second.untouched).toEqual(key.untouched)
    expect(second.widget).toEqual(key.widget)
    expect(second.actions).toEqual([obs(command)])
    expect(second.image).not.toBe(key.image)
    expect(second.widget).not.toBe(key.widget)
    expect((second.actions[0] as Extract<Action, { type: 'obs' }>).command).not.toBe(command)
  })

  test('the preview scene is green', () => {
    const key = draft()
    followInKey(key, { kind: 'scene', scene: 'Cámara', preview: true })
    expect(key.second!.from).toBe('#16a34a')
  })

  test('does nothing without a state or with a second state already', () => {
    const plain = draft()
    followInKey(plain, { kind: 'save_replay' })
    expect(plain.second).toBeNull()

    const existing: StateDraft = { ...draft(), actions: [], from: '#000000' }
    const key = { ...draft(), second: existing }
    followInKey(key, { kind: 'record', mode: 'toggle' })
    expect(key.second).toBe(existing)
    expect(existing.from).toBe('#000000')
  })
})
