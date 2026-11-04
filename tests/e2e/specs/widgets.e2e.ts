import { $, $$, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import type { LayerConfig, Widget } from '../../../src-nuxt/app/ipc/bindings'
import {
  button,
  chooseOption,
  chooseType,
  clickOn,
  done,
  fill,
  homePage,
  node,
  pngSize,
  saved,
  settingsDialog,
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

async function keyImage(): Promise<string> {
  const snapshot = await state()
  return browser.execute(async (page, index) => {
    const bytes = new Uint8Array(await (await fetch(`deck://localhost/key/${page}/${index}?live=${Date.now()}`)).arrayBuffer())
    let binary = ''
    bytes.forEach((b) => { binary += String.fromCharCode(b) })
    return btoa(binary)
  }, snapshot.home_page, WIDE_KEY)
}

async function keyImageSize() {
  return pngSize(Buffer.from(await keyImage(), 'base64'))
}

describe('live content on the wide key', () => {
  before(async () => {
    await clickOn(node('Inicio').$(`button[aria-label="Tecla ${WIDE_KEY + 1}"]`))
    await expect($('//*[normalize-space()="Ventana ancha · Inicio"]')).toBeDisplayed()
  })

  it('makes the key a clock under its text and keeps the preview ticking', async () => {
    await chooseType('widget:digital_clock')
    await settingsDialog()
    await saved()
    expect(homePage(await state()).keys[WIDE_KEY]!.kind).toBe('widget')
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
    await expect($('[aria-label="Información en vivo"]')).not.toBeExisting()
    await done()
    await chooseType('widget:system_monitor')
    await settingsDialog()
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

  it('switches to another live content and shows the network as a graph', async () => {
    await done()
    await chooseType('widget:analog_clock')
    await settingsDialog()
    await saved()
    expect(await widget()).toMatchObject({ kind: 'analog_clock', seconds: true, date: true })

    await done()
    await chooseType('widget:network')
    await settingsDialog()
    await saved()
    await toggle('Gráfica')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'network', graph: true, minutes: 5 })
    expect(await keyImageSize()).toEqual([452, 196])
    await shot('widget-network-graph')
  })

  it('counts down to a date with its own label', async () => {
    await done()
    await chooseType('widget:countdown')
    await settingsDialog()
    await fill($('[aria-label="Fecha y hora de la cuenta atrás"]'), '2099-12-31T23:59')
    await fill($('[aria-label="Etiqueta de la cuenta atrás"]'), 'Fin de siglo')
    await saved()
    expect(await widget()).toMatchObject({ kind: 'countdown', target: '2099-12-31T23:59', label: 'Fin de siglo' })
  })

  it('shows several live contents as a carousel, the next one on each press', async () => {
    const kinds = async () => (await wideLayers()).flatMap(l => (l.type === 'widget' ? [l.widget.kind] : []))
    await done()
    await chooseType('carousel')
    await settingsDialog()
    await saved()
    expect(homePage(await state()).keys[WIDE_KEY]!.kind).toBe('carousel')
    expect(await kinds()).toEqual(['digital_clock', 'calendar'])
    await expect($('[data-key-type-name]')).toHaveText('Carrusel')

    await clickOn($('//button[@role="tab"][normalize-space()="2. Calendario"]'))
    await chooseOption('Información en vivo', 'Fase lunar')
    await saved()
    expect(await kinds()).toEqual(['digital_clock', 'moon'])
    await clickOn(button('Añadir otra información en vivo'))
    await saved()
    expect(await kinds()).toHaveLength(3)
    await expect($$('[data-widget-tab]')).toBeElementsArrayOfSize(3)
    await shot('widget-carousel')

    await clickOn($('[aria-label="Información en vivo"]'))
    await expect($('//*[@role="option"][normalize-space()="Temporizador"][@data-disabled]')).toExist()
    await browser.keys(Key.Escape)
    await clickOn($$('[data-widget-tab] button[aria-label^="Quitar"]')[2]!)
    await saved()
    expect(await kinds()).toEqual(['digital_clock', 'moon'])
    await done()

    const clock = await keyImage()
    await clickOn(button('Probar tecla'))
    await browser.waitUntil(async () => (await keyImage()) !== clock, {
      timeout: 4000,
      timeoutMsg: 'a press did not move on to the moon',
    })
    await shot('widget-several')

    await $('[data-configure-type]').click()
    await settingsDialog()
    await clickOn(button('Quitar Fase lunar'))
    await saved()
    expect(await kinds()).toEqual(['digital_clock'])
    await expect($$('[data-widget-tab]')).toBeElementsArrayOfSize(1)
    await expect($$('[data-widget-tab] button[aria-label^="Quitar"]')).toBeElementsArrayOfSize(0)
  })

  it('leaves the rest of the key when it changes type', async () => {
    await done()
    await chooseType('basic')
    await saved()
    expect(homePage(await state()).keys[WIDE_KEY]!.kind).toBe('basic')
    expect((await wideLayers()).map(l => l.type)).toEqual(['text'])
    await expect(button('Vaciar tecla')).toBeDisplayed()
  })
})
