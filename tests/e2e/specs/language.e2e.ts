import { $, browser, expect } from '@wdio/globals'
import type { Preferences } from '../../../src-nuxt/app/ipc/bindings'
import { KEY_COUNT, button, chooseOption, clickOn, engine, homePage, node, shot, state } from '../helpers'

async function tab(name: string) {
  await clickOn($(`//*[@role="tab"][normalize-space()="${name}"]`))
  await $(`//*[@role="tab"][normalize-space()="${name}"][@aria-selected="true"]`).waitForExist()
}

describe('the app in English', () => {
  after(async () => {
    const preferences: Preferences = { ...(await state()).preferences, language: 'system' }
    await engine('set_preferences', { preferences })
  })

  it('switches to English from the settings', async () => {
    await $('a[href="/settings"]').click()
    await chooseOption('Idioma', 'English')
    await browser.waitUntil(async () => (await state()).language === 'en', { timeoutMsg: 'the engine did not switch' })
    await expect($('h1=Settings')).toBeDisplayed()
    await expect($('[aria-label="Language"]')).toBeExisting()
    await shot('en-settings-general')
    const tabs = [['Dock', 'dock'], ['Presses', 'presses'], ['New keys', 'new-keys'], ['Lock and idle', 'idle'], ['Profiles', 'profiles'], ['Integrations', 'integrations']]
    for (const [name, file] of tabs) {
      await tab(name!)
      await shot(`en-settings-${file}`)
    }
  })

  it('shows the editor and its catalogs in English', async () => {
    await clickOn($('[data-profile][aria-current="true"]'))
    const home = homePage(await state())
    const empty = [...Array(KEY_COUNT).keys()].reverse().find(i => !home.keys[i])!
    await clickOn(node(home.name).$('button[aria-label="Key 1"]'))
    await shot('en-inspector')
    await clickOn(node(home.name).$(`button[aria-label="Key ${empty + 1}"]`))
    await button('Set up key').click()
    await expect(button('Add action')).toBeDisplayed()
    await shot('en-new-key')
    await button('Add action').click()
    await shot('en-action-picker')
    await browser.keys('Escape')
    await button('Add plugin').click()
    await shot('en-plugin-menu')
    await browser.keys('Escape')
  })
})
