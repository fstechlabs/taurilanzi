import { $, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import type { Action, Widget } from '../../../src-nuxt/app/ipc/bindings'
import { FakeHass } from '../fake-hass'
import {
  KEY_COUNT,
  addAction,
  button,
  clickOn,
  dialog,
  homePage,
  node,
  saved,
  shot,
  state,
  text,
} from '../helpers'

const HASS_PORT = 39074
const TOKEN = 'token-e2e'

const hass = new FakeHass(HASS_PORT, TOKEN, {
  'cover.salon': { state: 'open', attributes: { friendly_name: 'Cortina del salón', device_class: 'curtain', current_position: 40 } },
  'switch.enchufe': { state: 'on', attributes: { friendly_name: 'Enchufe del café', device_class: 'outlet' }, device: 'enchufe' },
  'sensor.enchufe_potencia': { state: '35.2', attributes: { friendly_name: 'Potencia del enchufe', device_class: 'power', unit_of_measurement: 'W' }, device: 'enchufe' },
  'climate.salon': { state: 'heat', attributes: { friendly_name: 'Termostato', current_temperature: 20, temperature: 22, min_temp: 10, max_temp: 30 } },
})

async function emptyKeys(count: number) {
  const home = homePage(await state())
  return [...Array(KEY_COUNT).keys()].reverse().filter(i => !home.keys[i]).slice(0, count)
}

async function storedKey(index: number) {
  return homePage(await state()).keys[index]
}

async function editKey(index: number) {
  await clickOn(node(homePage(await state()).name).$(`button[aria-label="Tecla ${index + 1}"]`))
  await button('Configurar tecla').click()
}

async function until(check: () => Promise<boolean>, timeoutMsg: string) {
  await browser.waitUntil(check, { timeoutMsg, timeout: 15_000 })
}

/** The live content the key shows, once it is of `kind`. */
async function widgetOf<K extends Widget['kind']>(index: number, kind: K) {
  let found: Extract<Widget, { kind: K }> | undefined
  await until(async () => {
    const layer = (await storedKey(index))?.appearance.layers.find(l => l.type === 'widget')
    found = layer?.type === 'widget' && layer.widget.kind === kind ? layer.widget as Extract<Widget, { kind: K }> : undefined
    return Boolean(found)
  }, `the key does not show a ${kind}`)
  return found!
}

/** Picks an entity in the entity field named `label`, found by searching its name. */
async function chooseEntity(label: string, search: string, entity: string) {
  const field = $(`button[data-hass-entity][aria-label="${label}"]`)
  await field.scrollIntoView({ block: 'center' })
  await clickOn(field)
  const input = $('input[aria-label="Busca por nombre, área o id"]')
  await input.waitForDisplayed()
  await input.setValue(search)
  const item = $(`[data-entity="${entity}"]`)
  await item.waitForDisplayed()
  await clickOn(item)
  // A click may pick it already; Enter picks the one it focused otherwise.
  await browser.pause(200)
  if (await item.isExisting()) {
    await browser.keys(Key.Enter)
  }
  await item.waitForExist({ reverse: true })
  await expect($('[data-action-dialog]')).toBeDisplayed()
}

/** Picks an option of a select of the action's dialog: the inspector behind it has the same. */
async function chooseInDialog(trigger: string, option: string) {
  await clickOn(dialog().$(`[aria-label="${trigger}"]`))
  const item = $(`//*[@role="option"][normalize-space()="${option}"]`)
  await item.waitForDisplayed()
  await clickOn(item)
  await browser.keys(Key.Enter)
  await item.waitForExist({ reverse: true })
}

async function closeAction() {
  await button('Hecho', dialog()).click()
  await $('[data-action-dialog]').waitForExist({ reverse: true })
}

describe('Home Assistant', () => {
  let keys: number[]
  let server: string

  before(async () => {
    await hass.start()
    keys = await emptyKeys(3)
  })

  after(async () => {
    await hass.stop()
  })

  it('draws a curtain as open as it is on a smart tile', async () => {
    await editKey(keys[0]!)
    await addAction('home_assistant:tile')
    await button('Añadir servidor de Home Assistant').click()
    const add = dialog('Nuevo servidor de Home Assistant')
    await add.$('[aria-label="Dirección"]').setValue(hass.url)
    await add.$('[aria-label="Token de acceso"]').setValue(TOKEN)
    await button('Guardar', add).click()
    await until(async () => (await state()).integrations.some(i => i.kind === 'home_assistant'), 'the server was not added')
    server = (await state()).integrations.find(i => i.kind === 'home_assistant')!.id
    await until(async () => (await state()).hass[server]?.status.state === 'connected', 'Home Assistant did not connect')
    await button('Cerrar', add).click()
    await add.waitForExist({ reverse: true })

    await chooseEntity('Entidad', 'Cortina', 'cover.salon')
    const tile = await widgetOf(keys[0]!, 'hass_tile')
    expect(tile).toMatchObject({ server, entity: 'cover.salon', drawing: 'auto', extra: null })
    await expect(dialog().$('[data-look="tile"]')).toHaveAttribute('data-state', 'on')
    await expect(dialog().$('[aria-label="Dibujo"]')).toHaveText(expect.stringContaining('Automático: Cortina'))

    await chooseInDialog('Dibujo', 'Persiana')
    await expect($('[data-action-dialog]')).toBeDisplayed()
    await button('Nada', dialog()).click()
    await shot('hass-tile')
    await closeAction()
    await saved()
    const key = (await storedKey(keys[0]!))!
    expect(key.actions).toEqual([{ type: 'home_assistant', server, command: { kind: 'tile', entity: 'cover.salon', press: 'nothing' } }])
    expect(await widgetOf(keys[0]!, 'hass_tile')).toMatchObject({ drawing: 'shutter' })
  })

  it('shows what a plug draws and switches it when pressed', async () => {
    await editKey(keys[1]!)
    await addAction('home_assistant:tile')
    await chooseEntity('Entidad', 'Enchufe', 'switch.enchufe')
    expect(await widgetOf(keys[1]!, 'hass_tile')).toMatchObject({ entity: 'switch.enchufe', extra: 'sensor.enchufe_potencia' })
    await expect($('//button[normalize-space()="Encender / Apagar"]')).toHaveAttribute('data-state', 'on')
    await closeAction()
    await saved()
    expect((await storedKey(keys[1]!))!.actions).toEqual([
      { type: 'home_assistant', server, command: { kind: 'tile', entity: 'switch.enchufe', press: 'auto' } },
    ])

    await button('Probar tecla').waitForEnabled()
    await button('Probar tecla').click()
    await until(async () => hass.calls.some(c => c.domain === 'switch' && c.service === 'toggle'), 'the plug was not switched')
    expect(hass.calls.at(-1)!.target).toEqual({ entity_id: 'switch.enchufe' })
  })

  it('only shows a thermostat as a tile, without asking Home Assistant for anything', async () => {
    await editKey(keys[2]!)
    await addAction('home_assistant:show')
    await chooseEntity('Entidad', 'Termostato', 'climate.salon')
    expect(await widgetOf(keys[2]!, 'hass_tile')).toMatchObject({ entity: 'climate.salon', drawing: 'auto' })
    await expect(dialog().$('[data-look="tile"]')).toHaveAttribute('data-state', 'on')
    await expect(dialog().$('[aria-label="Dibujo"]')).toHaveText(expect.stringContaining('Dial'))
    await shot('hass-display-only')
    await clickOn(dialog().$('[data-look="card"]'))
    expect(await widgetOf(keys[2]!, 'hass_entity')).toMatchObject({ entity: 'climate.salon' })
    await clickOn(dialog().$('[data-look="tile"]'))
    await widgetOf(keys[2]!, 'hass_tile')
    await closeAction()
    await saved()
    const show: Action = { type: 'home_assistant', server, command: { kind: 'show', entity: 'climate.salon' } }
    expect((await storedKey(keys[2]!))!.actions).toEqual([show])

    const before = hass.calls.length
    await button('Probar tecla').waitForEnabled()
    await button('Probar tecla').click()
    await browser.pause(1000)
    expect(hass.calls.length).toBe(before)
    await expect(text('No se pudo probar la tecla.')).not.toBeExisting()
  })
})
