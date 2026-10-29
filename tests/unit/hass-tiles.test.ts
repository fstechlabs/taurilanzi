import { describe, expect, test } from 'bun:test'
import type { HassEntityInfo } from '../../src-nuxt/app/ipc/bindings'
import {
  drawingHasRange,
  followedState,
  hassError,
  newCommand,
  suggestedExtra,
  suitedDrawing,
  tileAction,
  tileDrawings,
} from '../../src-nuxt/app/lib/actions/hass'
import { newWidget } from '../../src-nuxt/app/lib/widgets'

function info(id: string, state: string, attributes: Record<string, unknown> = {}, device: string | null = null): HassEntityInfo {
  return { id, name: id, state, attributes, area: null, device, platform: null, translation_key: null, icon: null, secondary: false }
}

describe('tileAction', () => {
  test('says what a press does in the words of each domain', () => {
    expect(tileAction('light.salon')).toBe('Encender / Apagar')
    expect(tileAction('cover.salon')).toBe('Abrir / Cerrar')
    expect(tileAction('lock.door')).toBe('Bloquear / Desbloquear')
    expect(tileAction('scene.movie')).toBe('Activar')
    expect(tileAction('counter.coffee')).toBe('Sumar uno')
    expect(tileAction('media_player.tv')).toBe('Reproducir o pausar')
    expect(tileAction('input_select.mode')).toBe('Siguiente opción')
  })

  test('entities that only measure do nothing', () => {
    for (const id of ['sensor.temp', 'binary_sensor.door', 'weather.home', 'update.core', 'person.ana']) {
      expect(tileAction(id)).toBeNull()
    }
  })
})

describe('suitedDrawing', () => {
  test('follows the domain and the device class, as the engine does', () => {
    expect(suitedDrawing('light.a')).toBe('bulb')
    expect(suitedDrawing('switch.a', info('switch.a', 'on', { device_class: 'outlet' }))).toBe('plug')
    expect(suitedDrawing('switch.a', info('switch.a', 'on'))).toBe('switch')
    expect(suitedDrawing('cover.a', info('cover.a', 'open', { device_class: 'curtain' }))).toBe('curtain')
    expect(suitedDrawing('cover.a', info('cover.a', 'open', { device_class: 'damper' }))).toBe('blind')
    expect(suitedDrawing('cover.a', info('cover.a', 'open'))).toBe('shutter')
    expect(suitedDrawing('climate.a')).toBe('gauge')
    expect(suitedDrawing('sensor.a', info('sensor.a', '21.5', { device_class: 'temperature' }))).toBe('thermometer')
    expect(suitedDrawing('sensor.a', info('sensor.a', '48', { device_class: 'humidity' }))).toBe('droplet')
    expect(suitedDrawing('sensor.a', info('sensor.a', '800', { device_class: 'carbon_dioxide' }))).toBe('gauge')
    expect(suitedDrawing('sensor.a', info('sensor.a', '35', { device_class: 'power' }))).toBe('icon')
    expect(suitedDrawing('sensor.a', info('sensor.a', 'abc', { device_class: 'temperature' }))).toBe('icon')
    expect(suitedDrawing('binary_sensor.a', info('binary_sensor.a', 'on', { device_class: 'garage_door' }))).toBe('garage')
    expect(suitedDrawing('vacuum.a')).toBe('auto')
  })
})

describe('tileDrawings', () => {
  test('offers what suits each kind of entity', () => {
    expect(tileDrawings('cover.a')).toEqual(expect.arrayContaining(['curtain', 'blind', 'shutter', 'shade', 'garage', 'awning']))
    expect(tileDrawings('switch.a')).toContain('plug')
    expect(tileDrawings('switch.a')).not.toContain('gauge')
    expect(tileDrawings('sensor.a', info('sensor.a', '21'))).toContain('graph')
    expect(tileDrawings('sensor.a', info('sensor.a', 'abc'))).toEqual(['icon', 'value'])
    expect(tileDrawings('scene.a')).toEqual(['icon', 'value'])
  })

  test('only gauges, thermometers and droplets take a range', () => {
    expect(drawingHasRange('gauge')).toBe(true)
    expect(drawingHasRange('thermometer')).toBe(true)
    expect(drawingHasRange('droplet')).toBe(true)
    expect(drawingHasRange('curtain')).toBe(false)
  })
})

describe('suggestedExtra', () => {
  const entities = [
    info('switch.plug', 'on', { device_class: 'outlet' }, 'plug'),
    info('sensor.plug_energy', '12', { device_class: 'energy' }, 'plug'),
    info('sensor.plug_power', '35', { device_class: 'power' }, 'plug'),
    info('sensor.other_power', '5', { device_class: 'power' }, 'other'),
    info('cover.salon', 'open', {}, 'blind'),
    info('sensor.blind_power', '1', { device_class: 'power' }, 'blind'),
  ]

  test('is what the same device draws', () => {
    expect(suggestedExtra('switch.plug', entities)).toBe('sensor.plug_power')
  })

  test('only for what draws power, and only with a device', () => {
    expect(suggestedExtra('cover.salon', entities)).toBeNull()
    expect(suggestedExtra('switch.loose', [info('switch.loose', 'on')])).toBeNull()
  })
})

describe('commands that show an entity', () => {
  test('start without an entity, which they need', () => {
    expect(newCommand('show')).toEqual({ kind: 'show', entity: '' })
    expect(newCommand('tile', 'light.a')).toEqual({ kind: 'tile', entity: 'light.a', press: 'auto' })
    expect(hassError(newCommand('show'))).toBe('Elige la entidad')
    expect(hassError(newCommand('tile'))).toBe('Elige la entidad')
    expect(hassError(newCommand('tile', 'light.a'))).toBeNull()
  })

  test('follow the state of entities that are on or off', () => {
    expect(followedState({ kind: 'tile', entity: 'light.a', press: 'nothing' })).toMatchObject({ entity: 'light.a', state: 'on' })
    expect(followedState({ kind: 'show', entity: 'binary_sensor.door' })).toMatchObject({ entity: 'binary_sensor.door' })
    expect(followedState({ kind: 'show', entity: 'sensor.temp' })).toBeNull()
    expect(followedState({ kind: 'tile', entity: 'scene.movie', press: 'auto' })).toBeNull()
  })

  test('a new tile widget starts with what suits its entity', () => {
    expect(newWidget('hass_tile', null)).toMatchObject({ kind: 'hass_tile', drawing: 'auto', extra: null, min: null, max: null, hours: 24 })
  })
})
