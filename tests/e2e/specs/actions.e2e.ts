import { $, $$, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import {
  KEY_COUNT,
  actionSummaries,
  addAction,
  button,
  chooseOption,
  done,
  engine,
  homePage,
  newKey,
  pickFile,
  saved,
  settingsDialog,
  shot,
  silentWav,
  state,
  text,
} from '../helpers'

const PICKER = '[aria-label="Buscar acción"]'

async function entries() {
  return await Promise.all(await $$('[data-entry]').map(e => e.getAttribute('data-entry')))
}

describe('the action catalog', () => {
  let index: number
  let homeName: string
  let home: string

  before(async () => {
    const snapshot = await state()
    const page = homePage(snapshot)
    home = page.id
    homeName = page.name
    index = [...Array(KEY_COUNT).keys()].reverse().find(i => !page.keys[i])!
    await newKey(homeName, index)
  })

  after(async () => {
    if (homePage(await state()).keys[index]) {
      await engine('clear_key', { page: home, index })
    }
  })

  it('finds actions by search, section and filters', async () => {
    await button('Añadir acción').click()
    await $(PICKER).waitForDisplayed()
    await browser.pause(400)
    await shot('actions-1-catalog')
    const all = await entries()
    expect(all).toContain('hotkey')
    expect(all).toContain('obs:record')
    expect(all).toContain('home_assistant:switch')
    // What draws the key can only be its type.
    for (const face of ['basic', 'home_assistant:tile', 'home_assistant:show', 'widget:digital_clock', 'toggle_sequences', 'switch_hotkey', 'page_indicator']) {
      expect(all).not.toContain(face)
    }

    await $(PICKER).setValue('volumen')
    const volume = await entries()
    expect(volume).toEqual(expect.arrayContaining(['media:volume_up', 'media:volume_down']))
    expect(volume).not.toContain('hotkey')
    await $(PICKER).setValue('musica')
    expect(await entries()).toContain('play_sound')

    await $(PICKER).setValue('')
    await $('[data-section="obs"]').click()
    const obs = await entries()
    expect(obs.every(id => id!.startsWith('obs:'))).toBe(true)
    await expect($('[data-entry="obs:record"]')).toHaveAttribute('aria-disabled', 'false')
    await $('[data-section="lights"]').click()
    await expect($('[data-entry="lights:switch"]')).toHaveAttribute('aria-disabled', 'true')
    await $('[aria-label="Mostrar próximamente"]').click()
    expect(await entries()).toEqual([])
    await $('[aria-label="Mostrar próximamente"]').click()

    await $('[data-section="all"]').click()
    await $('[aria-label="Sin permisos especiales"]').click()
    const free = await entries()
    expect(free).not.toContain('hotkey')
    expect(free).toContain('open_url')
    await shot('actions-2-filtered')
    await browser.keys(Key.Escape)
    await $(PICKER).waitForExist({ reverse: true })
  })

  it('edits shortcuts, waits, random picks, text and sounds, and saves them', async () => {
    await addAction('hotkey')
    let dialog = await settingsDialog()
    await chooseOption('Tecla del atajo', 'C')
    await dialog.$('[aria-label="Comando"]').click()
    await expect(dialog.$('[data-shortcut]')).toHaveText('⌘C')
    await done()

    await addAction('delay')
    await (await settingsDialog()).$('[aria-label="Segundos de espera"]').setValue('1.5')
    await done()

    await addAction('random')
    dialog = await settingsDialog()
    await button('Añadir acción al azar', dialog).click()
    await $(PICKER).waitForDisplayed()
    expect(await entries()).not.toContain('home')
    expect(await entries()).not.toContain('delay')
    await $('[data-entry="media:mute"]').click()
    await $(PICKER).waitForExist({ reverse: true })
    // Its own dialog opens over the one of the random action.
    await browser.waitUntil(async () => (await $$('[data-action-dialog]').length) === 2)
    await done()
    await addAction('stop_sounds', 'Añadir acción al azar')
    await expect(dialog.$$('[data-action-summary]')).toBeElementsArrayOfSize(2)
    await done()

    await addAction('type_text')
    dialog = await settingsDialog()
    await dialog.$('[aria-label="Texto a escribir"]').setValue('hola')
    await dialog.$('[aria-label="Pegar en vez de teclear"]').click()
    await done()

    await addAction('play_sound')
    dialog = await settingsDialog()
    await pickFile(dialog.$('[aria-label="Archivo de sonido"]'), silentWav(), 'silencio.wav', 'audio/wav')
    await expect(text('silencio.wav', dialog)).toBeDisplayed()
    await chooseOption('Al pulsar otra vez', 'Pulsar para parar')
    await shot('actions-3-editors')
    await done()

    expect(await actionSummaries()).toEqual([
      'Pulsa el atajo ⌘C',
      'Espera 1,5 s',
      'Hace una de 2 acciones al azar',
      'Escribe «hola»',
      'Reproduce silencio.wav',
    ])

    await saved()
    const list = homePage(await state()).keys[index]!.actions
    expect(list.map(a => a.type)).toEqual(['hotkey', 'delay', 'random', 'type_text', 'play_sound'])
    expect(list[0]).toEqual({ type: 'hotkey', keys: { key: 'KeyC', command: true, shift: false, option: false, control: false } })
    expect(list[1]).toEqual({ type: 'delay', ms: 1500 })
    expect(list[2]).toEqual({ type: 'random', actions: [{ type: 'media', key: 'mute' }, { type: 'stop_sounds' }] })
    expect(list[3]).toMatchObject({ type: 'type_text', text: 'hola', paste: true })
    expect(list[4]).toMatchObject({ type: 'play_sound', name: 'silencio.wav', mode: 'toggle', volume: 80 })
  })

  it('tries a key without the dock', async () => {
    // Only a silent sound: the test app may inherit Accessibility from the terminal, and then
    // shortcuts and text would really be pressed and typed.
    const sound = homePage(await state()).keys[index]!.actions[4]!
    const key = homePage(await state()).keys[index]!
    await engine('set_key', { page: home, index, key: { ...key, actions: [sound] } })
    await button('Probar tecla').waitForEnabled()
    await button('Probar tecla').click()
    await expect(text('No se pudo probar la tecla.')).not.toBeExisting()
  })
})
