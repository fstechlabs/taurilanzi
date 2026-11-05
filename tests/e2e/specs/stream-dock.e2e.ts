import { join } from 'node:path'
import { $, browser, expect } from '@wdio/globals'
import type { PluginCatalog } from '../../../src-nuxt/app/ipc/bindings'
import { KEY_COUNT, button, clickOn, engine, homePage, node, saved, shot, state } from '../helpers'

const PLUGIN = 'com.taurilanzi.streamdock'
const COUNT = `${PLUGIN}.count`
const FIXTURE = join(import.meta.dirname, '..', 'fixtures', `${PLUGIN}.sdPlugin`)

async function catalog() {
  return await engine<PluginCatalog>('plugin_catalog')
}

async function until(check: () => Promise<boolean>, timeoutMsg: string, timeout = 15_000) {
  await browser.waitUntil(check, { timeout, timeoutMsg })
}

async function storedAction(index: number) {
  const action = homePage(await state()).keys[index]?.actions[0]
  return action?.type === 'plugin' ? action : null
}

async function keyImage(index: number) {
  const snapshot = await state()
  return await browser.execute(async (page, index) => {
    const bytes = new Uint8Array(await (await fetch(`deck://localhost/key/${page}/${index}?t=${Date.now()}`)).arrayBuffer())
    return bytes.join(',')
  }, snapshot.home_page, index)
}

describe('plugins made for Mirabox\'s Stream Dock', () => {
  let index: number

  before(async () => {
    expect(await engine<string>('install_plugin', { path: FIXTURE })).toBe(PLUGIN)
    await until(async () => (await catalog()).plugins.some(p => p.id === PLUGIN && p.connected), 'the plugin did not connect')
    const home = homePage(await state())
    index = [...Array(KEY_COUNT).keys()].reverse().find(i => !home.keys[i])!
    // A key that already has a type, which the plugin's replaces.
    await engine('set_key', {
      page: home.id,
      index,
      key: {
        kind: 'action',
        appearance: { background: { type: 'solid', color: { r: 30, g: 30, b: 30 } }, layers: [] },
        actions: [{ type: 'open_url', url: 'https://example.com' }],
        double_press: [],
        long_press: [],
        second_state: null,
      },
    })
  })

  after(async () => {
    if ((await catalog()).plugins.some(p => p.id === PLUGIN)) {
      await engine('uninstall_plugin', { id: PLUGIN })
    }
  })

  it('lists the plugin by its folder, in Spanish and running', async () => {
    const entry = $(`[data-plugin="${PLUGIN}"]`)
    await entry.waitForDisplayed({ timeout: 10_000 })
    await expect(entry).toHaveText('Contador Stream Dock')
    await entry.click()
    const details = $('[data-plugin-dialog]')
    await expect(details).toHaveText(expect.stringContaining('Contar'))
    await expect(details.$('[data-plugin-runtime]')).toHaveText('Node.js')
    await expect(details.$('[data-plugin-status]')).toHaveText('En marcha')
    await browser.keys('Escape')
    await details.waitForExist({ reverse: true })
  })

  it('turns a key of another type into its action, with the settings of the manifest', async () => {
    await clickOn(node(homePage(await state()).name).$(`button[aria-label="Tecla ${index + 1}"]`))
    await $('[data-change-type]').click()
    await $(`[data-section="plugin:${PLUGIN}"]`).click()
    await $(`[data-entry="plugin:${COUNT}"]`).click()
    await expect($('[data-plugin-inspector]')).toBeDisplayed()
    await saved()
    expect(await storedAction(index)).toMatchObject({ type: 'plugin', action: COUNT, settings: { step: 1 } })
    await expect($('[data-plugin-inspector]')).toBeDisplayed()
    await until(async () => ((await storedAction(index))?.settings as { inspector?: string })?.inspector === 'conectado', 'the inspector did not finish')
    expect((await storedAction(index))?.settings).toMatchObject({ step: 1, inspector: 'conectado', language: 'es', pong: 7, column: index % 5 })
    await shot('stream-dock-inspector')
  })

  it('sends presses to the plugin and draws what it answers', async () => {
    await button('Hecho').click()
    await $('[data-action-dialog]').waitForExist({ reverse: true })
    const before = await keyImage(index)
    for (let press = 1; press <= 2; press++) {
      await button('Probar tecla').waitForEnabled()
      await button('Probar tecla').click()
      await until(async () => ((await storedAction(index))?.settings as { count?: number })?.count === press, `press ${press} did not reach the plugin`)
    }
    expect(await keyImage(index)).not.toBe(before)
    expect((await storedAction(index))?.settings).toMatchObject({ count: 2, step: 1, inspector: 'conectado' })
    await shot('stream-dock-key')
  })
})
