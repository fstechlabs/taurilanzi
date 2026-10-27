import { $, $$, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import {
  KEY_COUNT,
  addAction,
  saved,
  setColor,
  assetBytes,
  button,
  center,
  clickOn,
  dialog,
  dragPoints,
  homePage,
  named,
  node,
  photoPng,
  pickFile,
  pngSize,
  shot,
  state,
  text,
} from '../helpers'

const PREVIEW = 'aside img[src^="deck://localhost/preview/"]'

async function previewSrc() {
  const image = $(PREVIEW)
  await expect(image).toBeDisplayed()
  return await image.getAttribute('src')
}

async function previewChange(previous: string | null) {
  await browser.waitUntil(async () => (await $(PREVIEW).getAttribute('src')) !== previous, {
    timeoutMsg: 'the preview did not change',
  })
  return await previewSrc()
}

async function storedKey(index: number) {
  return homePage(await state()).keys[index]
}

describe('the key editor', () => {
  let index: number
  let homeName: string

  before(async () => {
    const home = homePage(await state())
    homeName = home.name
    index = [...Array(KEY_COUNT).keys()].reverse().find(i => !home.keys[i])!
  })

  after(async () => {
    if (await storedKey(index)) {
      await browser.execute(
        (page, index) => (window as any).__TAURI_INTERNALS__.invoke('clear_key', { page, index }),
        (await state()).home_page,
        index,
      )
    }
  })

  it('creates a key with a gradient, an icon, a title and actions', async () => {
    await node(homeName).$(`button[aria-label="Tecla ${index + 1}"]`).click()
    await expect(text('Esta tecla está vacía.')).toBeDisplayed()
    await button('Configurar tecla').click()
    let src = await previewSrc()
    await shot('1-new')

    await text('Degradado').click()
    await setColor('Color inicial del degradado', '#ff0066')
    await setColor('Color final del degradado', '#3300cc')
    src = await previewChange(src)

    const picker = dialog()
    const results = () => picker.$$('button[aria-label*=":"]')
    await button('Buscar icono').click()
    const search = $('input[aria-label="Buscar icono"]')
    await search.setValue('github')
    await browser.waitUntil(async () => (await results().length) > 0, { timeout: 15_000 })
    const firstImage = (await results())[0]!.$('img')
    await browser.waitUntil(async () => await firstImage.getProperty('complete') === true, { timeout: 15_000 })
    expect(await firstImage.getProperty('naturalWidth')).toBeGreaterThan(0)
    await shot('2-icon-search')

    await text('Multicolor', picker).click()
    await browser.waitUntil(async () => (await results().length) > 0, { timeout: 15_000 })
    await text('Trazo', picker).click()
    await text('Cualquier estilo', picker).click()
    await button('Quitar filtros', picker).click()
    await expect(text('Cualquier color', picker)).toHaveAttribute('data-state', 'on')

    await search.setValue('logos:github-icon')
    await picker.$('button[aria-label="logos:github-icon"]').click()
    await expect(picker).not.toBeDisplayed()
    await expect(named('Color del icono')).not.toBeExisting()

    await button('Buscar icono').click()
    await expect(text('Usados hace poco', picker)).toBeDisplayed()
    await expect(picker.$('button[aria-label="logos:github-icon"]')).toBeDisplayed()
    await button('Colección', picker).click()
    await $('input[aria-label="Buscar colección"]').setValue('lucide')
    await $('//*[@role="option"][contains(normalize-space(), "Lucide")]').click()
    await expect(button('Colección', picker)).toHaveText(expect.stringContaining('Lucide'))
    await browser.waitUntil(async () => (await results().length) > 0, { timeout: 15_000 })
    const before = await results().length
    await picker.$('//button[starts-with(normalize-space(), "Cargar más")]').click()
    await browser.waitUntil(async () => (await results().length) > before, { timeout: 15_000 })
    await shot('2b-icon-browse')

    await search.setValue('mdi:home')
    await picker.$('button[aria-label="mdi:home"]').click()
    await expect(picker).not.toBeDisplayed()
    await expect(text('mdi:home')).toBeDisplayed()
    await setColor('Color del icono', '#00ff88')
    src = await previewChange(src)

    await named('Texto de la tecla').setValue('E2E')
    src = await previewChange(src)

    await clickOn($('[data-color-picker][aria-label="Color del texto"]'))
    await $('[data-swatch="#22c55e"]').click()
    await browser.pause(300)
    await shot('2c-color-picker')
    await browser.keys(Key.Escape)
    src = await previewChange(src)
    await setColor('Color del texto', '#ffffff')

    await addAction('open_url')
    const url = $('input[aria-label="Dirección"]')
    await url.setValue('ftp://example.org')
    await expect($$('//*[normalize-space()="Solo direcciones http:// o https://"]')[0]).toBeDisplayed()
    await expect($('[data-save-status]')).toHaveText(expect.stringContaining('Sin guardar'))
    await url.setValue('https://example.org')
    await addAction('home')
    await expect($('[aria-label="Acción 1: Ir al inicio"]')).toBeDisplayed()
    await button('Añadir acción').click()
    await expect($('[data-entry="back"]')).toHaveAttribute('aria-disabled', 'true')
    await browser.keys(Key.Escape)
    await $('[aria-label="Buscar acción"]').waitForExist({ reverse: true })
    const handle = await center($('[aria-label="Arrastrar para cambiar el orden de la acción 1"]'))
    const below = await center($('[aria-label="Acción 2: Abrir web"]'))
    await dragPoints(handle, { x: handle.x, y: below.y + 40 })
    await expect($('[aria-label="Acción 2: Ir al inicio"]')).toBeDisplayed()
    await shot('3-icon-edited')
    await saved()
    const stored = (await storedKey(index))!
    expect(stored.actions).toEqual([{ type: 'open_url', url: 'https://example.org' }, { type: 'home' }])
    expect(stored.appearance.background.type).toBe('linear_gradient')
    expect(stored.appearance.layers.map(l => l.type)).toEqual(['image', 'text'])
    const [icon, title] = stored.appearance.layers
    expect(icon).toMatchObject({ icon: 'mdi:home', tint: { r: 0, g: 255, b: 136 } })
    expect(title).toMatchObject({ content: 'E2E', position: 'bottom' })
    await expect(node(homeName).$(`button[aria-label="Tecla ${index + 1}"] img`)).toBeDisplayed()
  })

  it('edits an icon and keeps it vector with its tint', async () => {
    const editor = dialog('Editar imagen')
    const apply = button('Aplicar', editor)
    await button('Editar').click()
    await expect(apply).toBeEnabled()
    await button('Girar a la derecha', editor).click()
    await apply.click()
    await expect(editor).not.toBeDisplayed()
    await saved()
    const layer = (await storedKey(index))!.appearance.layers[0]!
    if (layer.type !== 'image') {
      throw new Error('the first layer should be the icon')
    }
    expect(layer.crop?.rotation).toBe(90)
    expect(layer.source).not.toBe(layer.asset)
    const svg = (await assetBytes(layer.asset)).toString()
    expect(svg.startsWith('<svg') && svg.includes('currentColor')).toBe(true)
    expect(layer).toMatchObject({ icon: 'mdi:home', tint: { r: 0, g: 255, b: 136 } })
  })

  it('frames an uploaded photo and keeps the original next to the edited copy', async () => {
    const photo = photoPng()
    const editor = dialog('Editar imagen')
    const apply = button('Aplicar', editor)
    let src = await previewSrc()

    await pickFile($('input[type=file]'), photo, 'photo.png', 'image/png')
    await expect(editor).toBeDisplayed()
    await expect(apply).toBeEnabled()
    const stage = editor.$('[aria-label="Encuadre de la imagen"]')
    const middle = await center(stage)
    await dragPoints(middle, { x: middle.x + 60, y: middle.y + 3 }, 6, true)
    await browser.action('wheel').scroll({ x: middle.x, y: middle.y, deltaY: -200 }).perform()
    await button('Voltear en horizontal', editor).click()
    await button('Girar a la derecha', editor).click()
    await editor.$('[aria-label="Enderezar"] [role="slider"]').click()
    await browser.keys([Key.ArrowRight, Key.ArrowRight, Key.ArrowRight])
    const zoomText = await editor.$('.//*[contains(text(), "%")]').getText()
    await shot('4-editor')
    await apply.click()
    await expect(editor).not.toBeDisplayed()
    await expect(text('Imagen editada')).toBeDisplayed()
    src = await previewChange(src)
    await saved()

    const layer = (await storedKey(index))!.appearance.layers[0]!
    if (layer.type !== 'image' || !layer.crop || !layer.source) {
      throw new Error('the first layer should be the edited photo')
    }
    expect(layer.asset).not.toBe(layer.source)
    expect(layer.crop.x).toBeLessThan(-0.1)
    expect(Math.abs(layer.crop.y)).toBeLessThan(0.01)
    expect(layer.crop.zoom).toBeGreaterThan(1)
    expect(layer.crop.flip_x).toBe(true)
    expect(layer.crop.rotation).toBe(91.5)
    expect((await assetBytes(layer.source)).equals(photo)).toBe(true)
    expect(pngSize(await assetBytes(layer.asset))).toEqual([392, 392])

    await button('Editar').click()
    await expect(apply).toBeEnabled()
    await expect(text(zoomText, editor)).toBeDisplayed()
    await expect(text('1.5°', editor)).toBeDisplayed()
    await button('Cancelar', editor).click()
    // Edits of one key a moment apart merge into one undo step; these are separate.
    await browser.pause(1100)
    await button('Volver a la original').click()
    await expect(text('Imagen editada')).not.toBeExisting()
    await saved()
    await button('Deshacer').click()
    await expect(text('Imagen editada')).toBeDisplayed()
    await shot('5-edited')
  })

  it('undoes and redoes edits, and clears the key', async () => {
    await browser.pause(1100)
    await named('Texto de la tecla').setValue('Cambio')
    await saved()
    await button('Deshacer').click()
    await expect(named('Texto de la tecla')).toHaveValue('E2E')
    await button('Rehacer').click()
    await expect(named('Texto de la tecla')).toHaveValue('Cambio')
    expect((await storedKey(index))!.appearance.layers.at(-1)).toMatchObject({ content: 'Cambio' })

    await button('Vaciar tecla').click()
    await expect(text('Esta tecla está vacía.')).toBeDisplayed()
    expect(await storedKey(index)).toBeUndefined()
  })
})
