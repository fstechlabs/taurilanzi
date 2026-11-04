import { $, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import {
  KEY_COUNT,
  addAction,
  button,
  chooseOption,
  clickOn,
  done,
  editAction,
  engine,
  homePage,
  newKey,
  node,
  saved,
  settingsDialog,
  shot,
  state,
  text,
} from '../helpers'

async function freeKeys(count: number) {
  const page = homePage(await state())
  return [...Array(KEY_COUNT).keys()].reverse().filter(i => !page.keys[i]).slice(0, count)
}

async function create(index: number, type: string) {
  await newKey(homePage(await state()).name, index, type)
}

async function stored(index: number) {
  return homePage(await state()).keys[index]
}

/** Sets the shortcut of the action of one state of the key. */
async function shortcutOfState(state: number, key: string) {
  await $(`[data-state-actions="${state}"] [data-action-summary]`).click()
  await settingsDialog()
  await chooseOption('Tecla del atajo', key)
  await done()
}

describe('states, gestures and page variables', () => {
  let keys: number[]
  let home: string

  before(async () => {
    home = (await state()).home_page
    keys = await freeKeys(3)
  })

  after(async () => {
    for (const index of keys) {
      if (await stored(index)) {
        await engine('clear_key', { page: home, index })
      }
    }
  })

  it('sets up two shortcuts that alternate, each in its own state', async () => {
    const index = keys[0]!
    await create(index, 'switch_hotkey')
    await expect($('[aria-label="Estado 2"]')).toBeDisplayed()
    await shortcutOfState(2, 'A')
    await shortcutOfState(1, 'B')
    await saved()
    await shot('states-1-two-shortcuts')
    const key = (await stored(index))!
    expect(key.kind).toBe('two_states')
    expect(key.actions).toEqual([{ type: 'hotkey', keys: { key: 'KeyB', command: false, shift: false, option: false, control: false } }])
    expect(key.second_state?.actions).toEqual([{ type: 'hotkey', keys: { key: 'KeyA', command: false, shift: false, option: false, control: false } }])
  })

  it('switches state on every press and shows it on the canvas', async () => {
    // Safe actions only: the test app may inherit Accessibility, and shortcuts would be pressed.
    const index = keys[0]!
    const key = (await stored(index))!
    await engine('set_key', {
      page: home,
      index,
      key: { ...key, actions: [{ type: 'stop_sounds' }], second_state: { ...key.second_state!, actions: [{ type: 'stop_sounds' }] } },
    })
    const inSecond = async () => (await state()).second_states.some(s => s.page === home && s.index === index)
    expect(await inSecond()).toBe(false)
    await engine('press_key', { page: home, index, gesture: 'press' })
    expect(await inSecond()).toBe(true)
    const image = await node(homePage(await state()).name).$(`button[aria-label="Tecla ${index + 1}"] img`)
    await expect(image).toHaveAttribute('src', expect.stringContaining('state=2'))
    await engine('press_key', { page: home, index, gesture: 'press' })
    expect(await inSecond()).toBe(false)
  })

  it('runs an action on a long press, chosen in the action itself, and then has no two states', async () => {
    const index = keys[1]!
    await create(index, 'basic')
    await addAction('stop_sounds')
    await editAction(1)
    await chooseOption('Cuando', 'Al mantener pulsada')
    await done()
    await expect($('[data-when="long_press"]')).toBeDisplayed()
    await $('[data-change-type]').click()
    await expect($('[data-entry="toggle_sequences"]')).toHaveAttribute('aria-disabled', 'true')
    await browser.keys(Key.Escape)
    await $('[aria-label="Buscar acción"]').waitForExist({ reverse: true })
    await saved()
    await addAction('stop_sounds')
    await editAction(1)
    await chooseOption('Cuando', 'Con doble pulsación')
    await done()
    await addAction('stop_sounds')
    await saved()
    await editAction(1)
    await clickOn($('[aria-label="Cuando"]'))
    await browser.pause(300)
    await shot('states-2b-when-options')
    await browser.keys(Key.Escape)
    await done()
    await shot('states-2-gestures')
    const key = (await stored(index))!
    expect(key.long_press).toEqual([{ type: 'stop_sounds' }])
    expect(key.double_press).toEqual([{ type: 'stop_sounds' }])
    expect(key.actions).toEqual([{ type: 'stop_sounds' }])

    await button('Probar tecla').click()
    await $('//*[@role="menuitem"][normalize-space()="Pulsación larga"]').click()
    await expect(text('No se pudo probar la tecla.')).not.toBeExisting()
  })

  it('shows the page in the text of a key', async () => {
    const index = keys[2]!
    await create(index, 'page_indicator')
    await saved()
    const key = (await stored(index))!
    expect(key.kind).toBe('page_indicator')
    expect(key.appearance.layers.find(l => l.type === 'text')).toMatchObject({ content: '{page_number}/{page_count}' })
    await expect($('[data-key-preview]')).toHaveAttribute('src', expect.stringContaining(`page=${home}`))
    await shot('states-3-indicator')

    await browser.pause(1100)
    await button('Deshacer').click()
    await browser.waitUntil(async () => (await stored(index))?.appearance.layers.find(l => l.type === 'text')?.content !== '{page_number}/{page_count}')
    await browser.keys(Key.Escape)
  })
})
