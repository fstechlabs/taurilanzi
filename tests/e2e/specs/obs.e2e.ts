import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { $, $$, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import {
  KEY_COUNT,
  addAction,
  button,
  clickOn,
  dialog,
  fill,
  homePage,
  node,
  saved,
  shot,
  state,
} from '../helpers'

const PASSWORD = 'secreto-e2e'

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
  await browser.waitUntil(check, { timeoutMsg })
}

describe('OBS', () => {
  let keys: number[]
  let server: string

  before(async () => {
    keys = await emptyKeys(2)
  })

  it('offers every action of the OBS plugin in the catalog', async () => {
    await editKey(keys[0]!)
    await button('Añadir acción').click()
    await $('[data-section="obs"]').click()
    const entries = await $$('[data-entry^="obs:"]')
    expect(entries.length).toBe(18)
    for (const entry of entries) {
      await expect(entry).toHaveAttribute('aria-disabled', 'false')
    }
    await shot('obs-catalog')
    await browser.keys(Key.Escape)
    await $('[aria-label="Buscar acción"]').waitForExist({ reverse: true })
  })

  it('records with a key that shows when OBS is recording', async () => {
    await addAction('obs:record')
    await expect($('//*[normalize-space()="No tienes ningún servidor de OBS Studio"]')).toBeDisplayed()
    await button('Añadir servidor de OBS Studio').click()
    const add = dialog('Nuevo servidor de OBS')
    await expect(add.$('[aria-label="Nombre"]')).toHaveValue('OBS')
    await button('Guardar', add).click()
    await until(async () => (await state()).integrations.some(i => i.kind === 'obs'), 'the server was not added')
    server = (await state()).integrations.find(i => i.kind === 'obs')!.id
    await button('Cerrar', add).click()
    await add.waitForExist({ reverse: true })
    await saved()
    const record = { type: 'obs', server, command: { kind: 'record', mode: 'toggle' } }
    expect((await storedKey(keys[0]!))!.actions).toEqual([record])
    await expect($('//*[contains(normalize-space(), "Puedes escribir los nombres a mano")]')).toBeDisplayed()

    await button('Mostrar en la tecla si está activo').click()
    await saved()
    const stored = (await storedKey(keys[0]!))!
    expect(stored.second_state?.actions).toEqual([record])
    expect(stored.second_state?.appearance.background).not.toEqual(stored.appearance.background)
    await expect($('//*[starts-with(normalize-space(), "La tecla sigue a OBS")]')).toBeDisplayed()
    await shot('obs-follow')
  })

  it('switches scenes by a name typed without OBS', async () => {
    await editKey(keys[1]!)
    await addAction('obs:scene')
    await expect($('[data-save-status]')).toHaveText(expect.stringContaining('elige la escena'))
    await fill($('input[aria-label="Escena"]'), 'Juego')
    await saved()
    expect((await storedKey(keys[1]!))!.actions).toEqual([
      { type: 'obs', server, command: { kind: 'scene', scene: 'Juego', preview: false } },
    ])
  })

  it('keeps the password out of the configuration', async () => {
    await $('a[href="/settings"]').click()
    await clickOn($('//*[@role="tab"][normalize-space()="Integraciones"]'))
    await button('Editar OBS').click()
    const edit = dialog('Servidor de OBS')
    await expect(edit.$('[aria-label="Servidor"]')).toHaveValue('127.0.0.1')
    await expect(edit.$('[aria-label="Puerto"]')).toHaveValue('4455')

    await edit.$('[aria-label="Contraseña"]').setValue(PASSWORD)
    await button('Guardar', edit).click()
    await until(async () => Boolean((await state()).obs[server]?.password_set), 'the password was not stored')
    const data = process.env.TAURILANZI_E2E_DATA!
    expect(readFileSync(join(data, 'secrets.json'), 'utf8')).toContain(PASSWORD)
    expect(readFileSync(join(data, 'config.json'), 'utf8')).not.toContain(PASSWORD)
    await shot('obs-settings')

    await button('Quitar del llavero', edit).click()
    await until(async () => !(await state()).obs[server]?.password_set, 'the password was not removed')
    await button('Cerrar', edit).click()
  })
})
