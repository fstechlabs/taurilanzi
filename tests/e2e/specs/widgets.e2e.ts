import { $, browser, expect } from '@wdio/globals'
import type { LayerConfig, Widget } from '../../../src-nuxt/app/ipc/bindings'
import {
  button,
  chooseOption,
  clickOn,
  fill,
  homePage,
  node,
  pngSize,
  saved,
  shot,
  state,
} from '../helpers'

const WIDE_KEY = 13
const PREVIEW = 'aside img[src^="deck://localhost/preview/"]'

async function wideLayers(): Promise<LayerConfig[]> {
  return homePage(await state()).keys[WIDE_KEY]!.appearance.layers
}

async function widget(): Promise<Widget | undefined> {
  const layer = (await wideLayers()).find(l => l.type === 'widget')
  return layer?.type === 'widget' ? layer.widget : undefined
}

async function toggle(name: string) {
  const item = $(`//button[@aria-label="Vista" or ancestor::*[@aria-label="Vista"]][normalize-space()="${name}"]`)
  await item.scrollIntoView({ block: 'center' })
  await clickOn(item)
}

async function keyImageSize() {
  const snapshot = await state()
  const base64 = await browser.execute(async (page, index) => {
    const bytes = new Uint8Array(await (await fetch(`deck://localhost/key/${page}/${index}?live=${Date.now()}`)).arrayBuffer())
    let binary = ''
    bytes.forEach((b) => { binary += String.fromCharCode(b) })
    return btoa(binary)
  }, snapshot.home_page, WIDE_KEY)
  return pngSize(Buffer.from(base64, 'base64'))
}

describe('live content on the wide key', () => {
  before(async () => {
    await clickOn(node('Inicio').$(`button[aria-label="Tecla ${WIDE_KEY + 1}"]`))
    await expect($('//*[normalize-space()="Ventana ancha · Inicio"]')).toBeDisplayed()
  })

  it('adds a clock under the key text and keeps the preview ticking', async () => {
    await chooseOption('Información en vivo', 'Reloj digital')
    await saved()
    expect((await wideLayers()).map(l => l.type)).toEqual(['widget', 'text'])
    expect(await widget()).toMatchObject({ kind: 'digital_clock', seconds: false, hour12: false })

    const toggleSeconds = $('[aria-label="Mostrar los segundos"]')
    await toggleSeconds.scrollIntoView({ block: 'center' })
    await toggleSeconds.click()
    await saved()
    expect(await widget()).toMatchObject({ kind: 'digital_clock', seconds: true })

    const first = await $(PREVIEW).getAttribute('src')
    await browser.waitUntil(async () => (await $(PREVIEW).getAttribute('src')) !== first, {
      timeout: 4000,
      timeoutMsg: 'the preview of a clock did not refresh',
    })
    expect(await keyImageSize()).toEqual([452, 196])
    await shot('widget-clock')
  })

  it('shows the system monitor as a graph over a period typed in minutes', async () => {
    await chooseOption('Información en vivo', 'Monitor del sistema')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'system_monitor', graph: false })

    await toggle('Gráfica')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'system_monitor', graph: true, minutes: 5 })

    const minutes = $('[aria-label="Minutos de la gráfica"]')
    await fill(minutes, '90')
    await saved()
    expect(await widget()).toMatchObject({ graph: true, minutes: 90 })
    await expect($('//*[normalize-space()="· 1 h 30 min"]')).toBeDisplayed()

    await fill(minutes, '5000')
    await saved()
    expect(await widget()).toMatchObject({ minutes: 1440 })
    await shot('widget-monitor-graph')

    await toggle('Barras')
    await saved()
    expect(await widget()).toMatchObject({ graph: false })
  })

  it('keeps the look when switching widgets and shows the network as a graph', async () => {
    await chooseOption('Información en vivo', 'Reloj analógico')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'analog_clock', seconds: true, date: true })

    await chooseOption('Información en vivo', 'Red')
    await saved()
    await toggle('Gráfica')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'network', graph: true, minutes: 5 })
    expect(await keyImageSize()).toEqual([452, 196])
    await shot('widget-network-graph')
  })

  it('counts down to a date with its own label', async () => {
    await chooseOption('Información en vivo', 'Cuenta atrás')
    await fill($('[aria-label="Fecha y hora de la cuenta atrás"]'), '2099-12-31T23:59')
    await fill($('[aria-label="Etiqueta de la cuenta atrás"]'), 'Fin de siglo')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'countdown', target: '2099-12-31T23:59', label: 'Fin de siglo' })
  })

  it('removes the live content and leaves the rest of the key', async () => {
    await chooseOption('Información en vivo', 'Ninguna')
    await saved()
    expect((await wideLayers()).map(l => l.type)).toEqual(['text'])
    await expect(button('Vaciar tecla')).toBeDisplayed()
  })
})
