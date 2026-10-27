import { execFileSync } from 'node:child_process'
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { $, browser, expect } from '@wdio/globals'
import type { PluginCatalog } from '../../../src-nuxt/app/ipc/bindings'
import { COMMUNITY_PORT, button, engine, menuItem, shot } from '../helpers'

const PLUGIN = 'com.taurilanzi.e2e.counter'
const FOLDER = 'com.taurilanzi.e2e.ulanziPlugin'
const FIXTURE = join(import.meta.dirname, '..', 'fixtures', FOLDER)
const BASE = `http://127.0.0.1:${COMMUNITY_PORT}`
const ICON = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#38bdf8"/></svg>'
const SHOT = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 9" width="320" height="180"><rect width="16" height="9" fill="#334155"/></svg>'

/** The package as a store serves it: a ZIP with the plugin folder at its root. */
function packageOf(version: string, work: string) {
  const dir = join(work, version)
  cpSync(FIXTURE, join(dir, FOLDER), { recursive: true })
  const manifest = join(dir, FOLDER, 'manifest.json')
  writeFileSync(manifest, readFileSync(manifest, 'utf8').replace('"Version": "1.0.0"', `"Version": "${version}"`))
  execFileSync('/usr/bin/zip', ['-qr', join(work, `${version}.zip`), FOLDER], { cwd: dir })
  return readFileSync(join(work, `${version}.zip`))
}

async function installed() {
  return (await engine<PluginCatalog>('plugin_catalog')).plugins.find(p => p.id === PLUGIN)
}

describe('plugins of the community catalog', () => {
  const work = mkdtempSync(join(tmpdir(), 'taurilanzi-community-'))
  const packages = { '1.0.0': packageOf('1.0.0', work), '1.1.0': packageOf('1.1.0', work) }
  let offered: keyof typeof packages = '1.0.0'
  const catalog = () => ({
    plugins: [
      {
        id: FOLDER,
        name: 'Counter',
        author: 'Taurilanzi',
        version: offered,
        description: 'Counts presses.',
        longDescription: 'Counts every press of its key.',
        i18n: { es: { name: 'Contador de la comunidad', description: 'Cuenta pulsaciones.' } },
        icon: `${BASE}/icon.svg`,
        screenshots: [`${BASE}/shot.svg`],
        deviceTypes: ['deck'],
        platforms: ['mac', 'windows'],
        tags: ['prueba'],
        downloads: 20,
        downloadUrl: `${BASE}/package.zip`,
        sourceUrl: 'https://github.com/taurilanzi/counter',
        security: { status: 'clean' },
      },
      {
        id: 'com.taurilanzi.risky.ulanziPlugin',
        name: 'Risky',
        author: 'Someone',
        version: '2.0.0',
        description: 'Downloads from anywhere.',
        deviceTypes: ['deck', 'dial'],
        platforms: ['mac'],
        downloads: 5,
        downloadUrl: 'https://downloads.example.com/risky.zip',
        security: { status: 'findings', high: 2 },
      },
      {
        id: 'com.taurilanzi.windows.ulanziPlugin',
        name: 'Only Windows',
        version: '1.0.0',
        platforms: ['windows'],
        downloadUrl: `${BASE}/package.zip`,
      },
      {
        id: 'com.taurilanzi.dial.ulanziPlugin',
        name: 'Only dials',
        version: '1.0.0',
        deviceTypes: ['dial'],
        downloadUrl: `${BASE}/package.zip`,
      },
    ],
  })
  const server = createServer((req, res) => {
    const files: Record<string, [string, string | Buffer]> = {
      '/catalog.json': ['application/json', JSON.stringify(catalog())],
      '/icon.svg': ['image/svg+xml', ICON],
      '/shot.svg': ['image/svg+xml', SHOT],
      '/package.zip': ['application/zip', packages[offered]],
    }
    const file = files[req.url ?? '']
    res.writeHead(file ? 200 : 404, { 'content-type': file?.[0] ?? 'text/plain' })
    res.end(file?.[1] ?? '')
  })
  const details = () => $('[data-community-details]')

  before(async () => {
    await new Promise<void>(resolve => server.listen(COMMUNITY_PORT, '127.0.0.1', resolve))
  })

  after(async () => {
    if (await installed()) {
      await engine('uninstall_plugin', { id: PLUGIN })
    }
    server.close()
    rmSync(work, { recursive: true, force: true })
  })

  it('lists only what runs on a Mac with keys, in Spanish', async () => {
    await button('Añadir plugin').click()
    await menuItem('Plugins de la comunidad…').click()
    const window = $('[data-community-dialog]')
    await expect(window).toBeDisplayed()
    const counter = $(`[data-community-plugin="${FOLDER}"]`)
    await counter.waitForDisplayed({ timeout: 15_000 })
    await expect(counter).toHaveText(expect.stringContaining('Contador de la comunidad'))
    await expect($('[data-community-plugin="com.taurilanzi.risky.ulanziPlugin"]')).toBeDisplayed()
    await expect($('[data-community-plugin="com.taurilanzi.windows.ulanziPlugin"]')).not.toBeExisting()
    await expect($('[data-community-plugin="com.taurilanzi.dial.ulanziPlugin"]')).not.toBeExisting()
    await expect(details()).toHaveText(expect.stringContaining('Cuenta pulsaciones.'))
    await browser.waitUntil(async () => await counter.$('img').getProperty('naturalWidth') as number > 0, { timeoutMsg: 'the icon did not load' })
    await shot('community-catalog')
  })

  it('warns about findings and only downloads from the stores\' hosts', async () => {
    await $('[data-community-plugin="com.taurilanzi.risky.ulanziPlugin"]').click()
    await expect(details()).toHaveText(expect.stringContaining('ha encontrado avisos'))
    await expect(details()).toHaveText(expect.stringContaining('diales'))
    await shot('community-warning')
    await button('Instalar', details()).click()
    await expect($('[data-toast]')).toHaveText(expect.stringContaining('downloads.example.com'))
  })

  it('installs a plugin in one click', async () => {
    await $(`[data-community-plugin="${FOLDER}"]`).click()
    await button('Instalar', details()).click()
    await details().$('[data-community-installed]').waitForDisplayed({ timeout: 20_000 })
    expect((await installed())?.version).toBe('1.0.0')
    await browser.keys('Escape')
    await $(`[data-plugin="${PLUGIN}"]`).waitForDisplayed()
  })

  it('offers a newer version and updates to it', async () => {
    offered = '1.1.0'
    await engine('community_plugins', { refresh: true })
    await $(`[data-plugin="${PLUGIN}"]`).click()
    const plugin = $('[data-plugin-dialog]')
    await button('Actualizar a 1.1.0', plugin).waitForDisplayed({ timeout: 10_000 })
    await shot('community-update')
    await button('Actualizar a 1.1.0', plugin).click()
    await browser.waitUntil(async () => (await installed())?.version === '1.1.0', { timeout: 20_000, timeoutMsg: 'the plugin was not updated' })
    await button('Actualizar a 1.1.0', plugin).waitForExist({ reverse: true })
    await browser.keys('Escape')
  })
})
