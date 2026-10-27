import { createServer } from 'node:http'
import { join } from 'node:path'
import { $, browser, expect } from '@wdio/globals'
import type { PluginCatalog } from '../../../src-nuxt/app/ipc/bindings'
import {
  KEY_COUNT,
  button,
  clickOn,
  engine,
  homePage,
  node,
  saved,
  shot,
  state,
} from '../helpers'

const PLUGIN = 'com.taurilanzi.e2e.counter'
const COUNT = `${PLUGIN}.count`
const FIXTURE = join(import.meta.dirname, '..', 'fixtures', 'com.taurilanzi.e2e.ulanziPlugin')
/** The inspector of the test plugin calls this server as it loads. */
const ECHO_PORT = 39071

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

describe('plugins made for Ulanzi Studio', () => {
  const received: { body: string, test: string | undefined }[] = []
  const echo = createServer((req, res) => {
    let body = ''
    req.on('data', (chunk) => {
      body += chunk
    })
    req.on('end', () => {
      received.push({ body, test: req.headers['x-test'] as string | undefined })
      res.writeHead(200, { 'content-type': 'text/plain' })
      res.end(`eco: ${body}`)
    })
  })
  let index: number

  before(async () => {
    await new Promise<void>(resolve => echo.listen(ECHO_PORT, '127.0.0.1', resolve))
    expect(await engine<string>('install_plugin', { path: FIXTURE })).toBe(PLUGIN)
    await until(async () => (await catalog()).plugins.some(p => p.id === PLUGIN && p.connected), 'the plugin did not connect')
    const home = homePage(await state())
    index = [...Array(KEY_COUNT).keys()].reverse().find(i => !home.keys[i])!
  })

  after(() => {
    echo.close()
  })

  it('lists the plugin in the sidebar, in Spanish and running', async () => {
    const entry = $(`[data-plugin="${PLUGIN}"]`)
    await entry.waitForDisplayed({ timeout: 10_000 })
    await expect(entry).toHaveText('Contador de prueba')
    await until(async () => (await entry.$('[title="En marcha"]').isExisting()), 'the plugin does not show as running')
    await entry.click()
    const details = $('[data-plugin-dialog]')
    await expect(details).toBeDisplayed()
    await expect(details).toHaveText(expect.stringContaining('Contar'))
    await expect(details).toHaveText(expect.stringContaining('Para diales u otros modelos'))
    await expect(details.$('[data-plugin-status]')).toHaveText('En marcha')
    await shot('plugin-details')
    await browser.keys('Escape')
    await details.waitForExist({ reverse: true })
  })

  it('offers its actions in the catalog and opens the inspector when one is added', async () => {
    await clickOn(node(homePage(await state()).name).$(`button[aria-label="Tecla ${index + 1}"]`))
    await button('Configurar tecla').click()
    await button('Añadir acción').click()
    await $(`[data-section="plugin:${PLUGIN}"]`).click()
    await expect($(`[data-entry="plugin:${PLUGIN}.dial"]`)).toHaveAttribute('aria-disabled', 'true')
    await shot('plugin-catalog')
    await $(`[data-entry="plugin:${COUNT}"]`).click()
    await expect($('[data-action-dialog]')).toBeDisplayed()
    await expect($('[data-plugin-inspector]')).toBeDisplayed()
    await saved()
    expect(await storedAction(index)).toMatchObject({ type: 'plugin', action: COUNT })
    expect(homePage(await state()).keys[index]!.appearance.layers).toEqual([])
  })

  it('lets the inspector talk to the plugin and reach other servers, as in Ulanzi Studio', async () => {
    await until(async () => ((await storedAction(index))?.settings as { pong?: string })?.pong === 'eco: hola', 'the inspector did not finish')
    expect((await storedAction(index))?.settings).toMatchObject({ inspector: 'conectado', echo: 'eco: hola', pong: 'eco: hola' })
    expect(received.at(-1)).toEqual({ body: 'hola', test: 'si' })
    await shot('plugin-inspector')
  })

  it('sends presses to the plugin and draws what it answers', async () => {
    await button('Hecho').click()
    await $('[data-action-dialog]').waitForExist({ reverse: true })
    const before = await keyImage(index)
    for (let press = 1; press <= 3; press++) {
      await button('Probar tecla').waitForEnabled()
      await button('Probar tecla').click()
      await until(async () => ((await storedAction(index))?.settings as { count?: number })?.count === press, `press ${press} did not reach the plugin`)
    }
    await expect($('[data-toast]')).toHaveText(expect.stringContaining('Tres pulsaciones'))
    expect(await keyImage(index)).not.toBe(before)
    expect((await storedAction(index))?.settings).toMatchObject({ count: 3, inspector: 'conectado' })
    await shot('plugin-key')
  })

  it('removes a plugin installed in the app', async () => {
    await $(`[data-plugin="${PLUGIN}"]`).click()
    const details = $('[data-plugin-dialog]')
    await button('Quitar', details).click()
    await button('¿Quitar el plugin?', details).click()
    await $(`[data-plugin="${PLUGIN}"]`).waitForExist({ reverse: true })
    expect((await catalog()).plugins.some(p => p.id === PLUGIN)).toBe(false)
    await expect($(`[aria-label="Acción 1: Acción de un plugin no instalado"]`)).toBeDisplayed()
  })
})
