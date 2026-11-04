import { execFileSync } from 'node:child_process'
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { $, $$, browser, expect } from '@wdio/globals'
import type { PluginCatalog } from '../../../src-nuxt/app/ipc/bindings'
import { STORE_PORT, button, engine, menuItem, shot } from '../helpers'

const PLUGIN = 'com.taurilanzi.e2e.counter'
const FOLDER = 'com.taurilanzi.e2e.ulanziPlugin'
const PYTHON = 'com.taurilanzi.e2e.python'
const PYTHON_FOLDER = 'com.taurilanzi.e2e.python.ulanziPlugin'
const FIXTURES = join(import.meta.dirname, '..', 'fixtures')
const BASE = `http://127.0.0.1:${STORE_PORT}`
const ICON = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#38bdf8"/></svg>'
const SHOT = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 9" width="320" height="180"><rect width="16" height="9" fill="#334155"/></svg>'

/** The package as a store serves it: a ZIP with the plugin folder at its root. */
function packageOf(folder: string, version: string, work: string) {
  const dir = join(work, `${folder}-${version}`)
  cpSync(join(FIXTURES, folder), join(dir, folder), { recursive: true })
  const manifest = join(dir, folder, 'manifest.json')
  writeFileSync(manifest, readFileSync(manifest, 'utf8').replace('"Version": "1.0.0"', `"Version": "${version}"`))
  const zip = join(work, `${folder}-${version}.zip`)
  execFileSync('/usr/bin/zip', ['-qr', zip, folder], { cwd: dir })
  return readFileSync(zip)
}

async function installed(id: string) {
  return (await engine<PluginCatalog>('plugin_catalog')).plugins.find(p => p.id === id)
}

describe('plugins of the stores', () => {
  const work = mkdtempSync(join(tmpdir(), 'taurilanzi-store-'))
  const packages = { '1.0.0': packageOf(FOLDER, '1.0.0', work), '1.1.0': packageOf(FOLDER, '1.1.0', work) }
  const python = packageOf(PYTHON_FOLDER, '1.0.0', work)
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
  const marketplace = {
    succ: true,
    data: [
      {
        id: 7,
        os: ['mac', 'windows'],
        productName: 'Eco en Python',
        author: 'Taurilanzi',
        uuid: PYTHON,
        version: '1.0.0',
        popular: 40,
        classify: 4,
        contentType: 'Herramientas',
        overview: 'Se conecta desde Python.',
        device: ['D200'],
        iconUrl: `${BASE}/icon.svg`,
        fileUrl: `${BASE}/python.zip`,
      },
      {
        id: 8,
        os: ['mac', 'windows'],
        productName: 'Solo para el D100',
        uuid: 'com.taurilanzi.d100',
        version: '1.0.0',
        device: ['D100H'],
        fileUrl: `${BASE}/python.zip`,
      },
    ],
  }
  const server = createServer((req, res) => {
    const files: Record<string, [string, string | Buffer]> = {
      '/catalog.json': ['application/json', JSON.stringify(catalog())],
      '/marketplace/product/list': ['application/json', JSON.stringify(marketplace)],
      '/icon.svg': ['image/svg+xml', ICON],
      '/shot.svg': ['image/svg+xml', SHOT],
      '/package.zip': ['application/zip', packages[offered]],
      '/python.zip': ['application/zip', python],
    }
    const file = files[(req.url ?? '').split('?')[0]!]
    res.writeHead(file ? 200 : 404, { 'content-type': file?.[0] ?? 'text/plain' })
    res.end(file?.[1] ?? '')
  })
  const details = () => $('[data-store-details]')
  const count = () => $('[data-store-count]')

  before(async () => {
    await new Promise<void>(resolve => server.listen(STORE_PORT, '127.0.0.1', resolve))
  })

  after(async () => {
    for (const id of [PLUGIN, PYTHON]) {
      if (await installed(id)) {
        await engine('uninstall_plugin', { id })
      }
    }
    server.close()
    rmSync(work, { recursive: true, force: true })
  })

  it('lists what runs on a Mac with a D200 from every store, in Spanish', async () => {
    await button('Añadir plugin').click()
    await menuItem('Tienda de plugins…').click()
    await expect($('[data-store-dialog]')).toBeDisplayed()
    const counter = $(`[data-store-plugin="${FOLDER}"]`)
    await counter.waitForDisplayed({ timeout: 15_000 })
    await expect(counter).toHaveText(expect.stringContaining('Contador de la comunidad'))
    await expect($('[data-store-plugin="ulanzi:7"]')).toHaveText(expect.stringContaining('Marketplace de Ulanzi'))
    await expect($('[data-store-plugin="com.taurilanzi.risky.ulanziPlugin"]')).toBeDisplayed()
    await expect($('[data-store-plugin="ulanzi:8"]')).not.toBeExisting()
    await expect($('[data-store-plugin="com.taurilanzi.windows.ulanziPlugin"]')).not.toBeExisting()
    await expect($('[data-store-plugin="com.taurilanzi.dial.ulanziPlugin"]')).not.toBeExisting()
    await expect(count()).toHaveText('3 plugins')
    const order = await $$('[data-store-plugin]').map(p => p.getAttribute('data-store-plugin'))
    expect(order).toEqual(['ulanzi:7', FOLDER, 'com.taurilanzi.risky.ulanziPlugin'])
    await browser.waitUntil(async () => await counter.$('img').getProperty('naturalWidth') as number > 0, { timeoutMsg: 'the icon did not load' })
    await shot('store-catalog')
  })

  it('filters by store, category and words, counting each choice', async () => {
    await expect($('[data-store-source="ulanzi"]')).toHaveText(expect.stringContaining('1'))
    await expect($('[data-store-source="community"]')).toHaveText(expect.stringContaining('2'))
    await $('[data-store-source="community"]').click()
    await expect(count()).toHaveText('2 plugins')
    await expect($('[data-store-plugin="ulanzi:7"]')).not.toBeExisting()
    await expect($('[data-store-kind="tools"]')).toBeDisabled()
    await $('[data-store-source="all"]').click()
    await $('[data-store-kind="tools"]').click()
    await expect(count()).toHaveText('1 plugin')
    await shot('store-filtered')
    await button('Quitar filtros').click()
    await $('[aria-label="Buscar plugins"]').setValue('pulsaciones')
    await expect(count()).toHaveText('1 plugin')
    await button('Quitar filtros').click()
    await expect(count()).toHaveText('3 plugins')
  })

  it('warns about findings and only downloads from the stores\' hosts', async () => {
    await $('[data-store-plugin="com.taurilanzi.risky.ulanziPlugin"]').click()
    await expect(details()).toHaveText(expect.stringContaining('ha encontrado avisos'))
    await expect(details()).toHaveText(expect.stringContaining('diales'))
    await expect(details().$('[data-store-origin]')).toHaveText('Ulanzi Community Store')
    await shot('store-warning')
    await button('Instalar', details()).click()
    await expect($('[data-toast]')).toHaveText(expect.stringContaining('downloads.example.com'))
  })

  it('installs a Python plugin of Ulanzi\'s Marketplace, which connects', async () => {
    await $('[data-store-plugin="ulanzi:7"]').click()
    await expect(details().$('[data-store-origin]')).toHaveText('Marketplace de Ulanzi')
    await button('Instalar', details()).click()
    await details().$('[data-store-installed]').waitForDisplayed({ timeout: 20_000 })
    await browser.waitUntil(async () => (await installed(PYTHON))?.connected === true, { timeout: 20_000, timeoutMsg: 'the Python plugin did not connect' })
    expect((await installed(PYTHON))?.runtime).toBe('python')
    await expect($('[data-store-status="installed"]')).toHaveText(expect.stringContaining('1'))
  })

  it('installs a plugin of the community in one click', async () => {
    await $(`[data-store-plugin="${FOLDER}"]`).click()
    await button('Instalar', details()).click()
    await details().$('[data-store-installed]').waitForDisplayed({ timeout: 20_000 })
    expect((await installed(PLUGIN))?.version).toBe('1.0.0')
    await browser.keys('Escape')
    await $(`[data-plugin="${PLUGIN}"]`).waitForDisplayed()
    await $(`[data-plugin="${PYTHON}"]`).click()
    await expect($('[data-plugin-dialog] [data-plugin-runtime]')).toHaveText('Python')
    await browser.keys('Escape')
  })

  it('offers a newer version and updates to it', async () => {
    offered = '1.1.0'
    await engine('store_plugins', { refresh: true })
    await $(`[data-plugin="${PLUGIN}"]`).click()
    const plugin = $('[data-plugin-dialog]')
    await button('Actualizar a 1.1.0', plugin).waitForDisplayed({ timeout: 10_000 })
    await shot('store-update')
    await button('Actualizar a 1.1.0', plugin).click()
    await browser.waitUntil(async () => (await installed(PLUGIN))?.version === '1.1.0', { timeout: 20_000, timeoutMsg: 'the plugin was not updated' })
    await button('Actualizar a 1.1.0', plugin).waitForExist({ reverse: true })
    await browser.keys('Escape')
  })
})
