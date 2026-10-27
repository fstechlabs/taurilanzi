import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { deflateSync } from 'node:zlib'
import { $, browser } from '@wdio/globals'
import type { ChainablePromiseElement } from 'webdriverio'
import { Key } from 'webdriverio'
import type { Page, Snapshot } from '../../src-nuxt/app/ipc/bindings'

const SCREENS = join(import.meta.dirname, 'screens')
export const KEY_COUNT = 13
/** The community catalog the app reads in the tests; the spec that needs it serves it. */
export const COMMUNITY_PORT = 39072

type Element = ChainablePromiseElement | WebdriverIO.Element

/** Asks the engine through the same IPC the UI uses. */
export async function engine<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return await browser.execute(
    (command, args) => (window as any).__TAURI_INTERNALS__.invoke(command, args),
    command,
    args,
  ) as T
}

export async function state() {
  return await engine<Snapshot>('get_state')
}

export function homePage(snapshot: Snapshot): Page {
  return pageById(snapshot, snapshot.home_page)
}

export function pageById(snapshot: Snapshot, id: string): Page {
  return snapshot.pages.find(p => p.id === id)!
}

/** An image from the store, as the UI loads it. */
export async function assetBytes(id: string): Promise<Buffer> {
  const base64 = await browser.execute(async (id) => {
    const bytes = new Uint8Array(await (await fetch(`deck://localhost/asset/${id}`)).arrayBuffer())
    let binary = ''
    bytes.forEach((b) => { binary += String.fromCharCode(b) })
    return btoa(binary)
  }, id)
  return Buffer.from(base64, 'base64')
}

export async function shot(name: string) {
  mkdirSync(SCREENS, { recursive: true })
  await browser.saveScreenshot(join(SCREENS, `e2e-${name}.png`))
}

/** Any element with this accessible name: aria-label, label or its own text. */
export function named(name: string, within?: Element) {
  return within ? (within as WebdriverIO.Element).$(`aria/${name}`) : $(`aria/${name}`)
}

/** The innermost element whose whole text is `text`. */
export function text(value: string, within?: Element) {
  const xpath = `.//*[normalize-space()="${value}" and not(*[normalize-space()="${value}"])]`
  return within ? (within as WebdriverIO.Element).$(xpath) : $(xpath)
}

export function menuItem(name: string) {
  return $(`//*[@role="menuitem"][normalize-space()="${name}" or @aria-label="${name}"]`)
}

/** The canvas card of a page, by name. */
export function node(name: string) {
  return $(`.vue-flow__node:has([data-page="${name}"])`)
}

/** Sets an input the way typing or a color picker would. */
export async function fill(input: Element, value: string) {
  await browser.execute((el: HTMLInputElement, value: string) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
    setter.call(el, value)
    el.dispatchEvent(new Event('input', { bubbles: true }))
    el.dispatchEvent(new Event('change', { bubbles: true }))
  }, await input as unknown as HTMLInputElement, value)
}

export async function center(element: Element) {
  await element.waitForExist()
  const { x, y, width, height } = await browser.getElementRect(await (await element).elementId)
  return { x: Math.round(x + width / 2), y: Math.round(y + height / 2) }
}

interface Point { x: number, y: number }

/**
 * Presses at `from`, moves to `to` in small steps and releases, firing the pointer and mouse
 * events a real mouse would: the WebDriver plugin only fakes mouse events, and the app listens
 * to pointer events. With `capture`, moves and the release go to the pressed element, as they
 * do after `setPointerCapture`. `on` presses that element instead of the one under `from`: in
 * WKWebView `elementFromPoint` never returns the SVG lines of the canvas, though the mouse hits them.
 */
export async function dragPoints(from: Point, to: Point, steps = 12, capture = false, on?: Element) {
  await browser.execute(async (from: Point, to: Point, steps: number, capture: boolean, on: Element | null) => {
    // Not requestAnimationFrame: WebKit stops it while the window is covered by another one.
    const frame = () => new Promise(resolve => setTimeout(resolve, 16))
    const at = (p: Point) => document.elementFromPoint(p.x, p.y) ?? document.body
    const fire = (target: Element, type: string, p: Point, buttons: number) => {
      const init = { bubbles: true, cancelable: true, composed: true, view: window, clientX: p.x, clientY: p.y, button: 0, buttons }
      target.dispatchEvent(new PointerEvent(`pointer${type}`, { ...init, pointerId: 1, pointerType: 'mouse', isPrimary: true }))
      target.dispatchEvent(new MouseEvent(`mouse${type}`, init))
    }
    const pressed = on ?? at(from)
    fire(pressed, 'down', from, 1)
    await frame()
    let last = from
    for (let i = 1; i <= steps; i++) {
      last = { x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps }
      fire(capture ? pressed : at(last), 'move', last, 1)
      await frame()
    }
    fire(capture ? pressed : at(last), 'up', last, 0)
    await frame()
  }, from, to, steps, capture, on ? await on as unknown as Element : null)
}

export async function drag(source: Element, target: Element, capture = false) {
  await dragPoints(await center(source), await center(target), 12, capture)
}

export async function dragBy(element: Element, dx: number, dy: number, capture = false) {
  const from = await center(element)
  await dragPoints(from, { x: from.x + dx, y: from.y + dy }, 10, capture, element)
}

/** A point `at` of the way along a canvas line, in viewport pixels. */
export async function pointOnLine(line: Element, at: number) {
  const path = await (line as WebdriverIO.Element).$('path.link-hit')
  return await browser.execute((path: SVGPathElement, at: number) => {
    const p = path.getPointAtLength(path.getTotalLength() * at)
    const m = path.getScreenCTM()!
    return { x: Math.round(p.x * m.a + p.y * m.c + m.e), y: Math.round(p.x * m.b + p.y * m.d + m.f) }
  }, path as unknown as SVGPathElement, at)
}

/** A full click at a point: down, up and click, or the right button and its context menu. */
export async function clickAt(point: Point, options: { button?: 'left' | 'right', double?: boolean, on?: Element } = {}) {
  await browser.execute(async (p: Point, right: boolean, double: boolean, on: Element | null) => {
    const target = on ?? document.elementFromPoint(p.x, p.y) ?? document.body
    const button = right ? 2 : 0
    const init = { bubbles: true, cancelable: true, composed: true, view: window, clientX: p.x, clientY: p.y, button }
    const press = (detail: number) => {
      target.dispatchEvent(new PointerEvent('pointerdown', { ...init, buttons: right ? 2 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true }))
      target.dispatchEvent(new MouseEvent('mousedown', { ...init, buttons: right ? 2 : 1, detail }))
      target.dispatchEvent(new PointerEvent('pointerup', { ...init, buttons: 0, pointerId: 1, pointerType: 'mouse', isPrimary: true }))
      target.dispatchEvent(new MouseEvent('mouseup', { ...init, buttons: 0, detail }))
      target.dispatchEvent(new MouseEvent(right ? 'contextmenu' : 'click', { ...init, detail }))
    }
    press(1)
    if (double) {
      press(2)
      target.dispatchEvent(new MouseEvent('dblclick', { ...init, detail: 2 }))
    }
    await new Promise(resolve => setTimeout(resolve, 16))
  }, point, options.button === 'right', Boolean(options.double), options.on ? await options.on as unknown as Element : null)
}

/** Clicks an element itself, even where something else covers its middle. */
export async function clickOn(element: Element, options: { button?: 'left' | 'right', double?: boolean } = {}) {
  await clickAt(await center(element), { ...options, on: element })
}

/** Hands a file to an `<input type=file>` as if the user had picked it. */
export async function pickFile(input: Element, bytes: Buffer, name: string, type: string) {
  await browser.execute((el: HTMLInputElement, base64: string, name: string, type: string) => {
    const binary = atob(base64)
    const data = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) {
      data[i] = binary.charCodeAt(i)
    }
    const transfer = new DataTransfer()
    transfer.items.add(new File([data], name, { type }))
    el.files = transfer.files
    el.dispatchEvent(new Event('change', { bubbles: true }))
  }, await input as unknown as HTMLInputElement, bytes.toString('base64'), name, type)
}

/** A landscape gradient, so cropping and rotation change what the key shows. */
export function photoPng(width = 320, height = 200): Buffer {
  const rows = Buffer.alloc((width * 3 + 1) * height)
  for (let y = 0; y < height; y++) {
    const row = y * (width * 3 + 1)
    for (let x = 0; x < width; x++) {
      rows[row + 1 + x * 3] = Math.floor((x * 255) / width)
      rows[row + 2 + x * 3] = Math.floor((y * 255) / height)
      rows[row + 3 + x * 3] = 160
    }
  }
  const chunk = (kind: string, data: Buffer) => {
    const body = Buffer.concat([Buffer.from(kind), data])
    const length = Buffer.alloc(4)
    length.writeUInt32BE(data.length)
    const crc = Buffer.alloc(4)
    crc.writeUInt32BE(crc32(body))
    return Buffer.concat([length, body, crc])
  }
  const header = Buffer.alloc(13)
  header.writeUInt32BE(width, 0)
  header.writeUInt32BE(height, 4)
  header.set([8, 2, 0, 0, 0], 8)
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(rows)),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

export function pngSize(data: Buffer) {
  return [data.readUInt32BE(16), data.readUInt32BE(20)]
}

function crc32(data: Buffer) {
  let crc = ~0
  for (const byte of data) {
    crc ^= byte
    for (let k = 0; k < 8; k++) {
      crc = (crc >>> 1) ^ (0xEDB88320 & -(crc & 1))
    }
  }
  return ~crc >>> 0
}

/** A button by its text or its aria-label. */
export function button(name: string, within?: Element) {
  const xpath = `.//button[normalize-space()="${name}" or @aria-label="${name}"]`
  return within ? (within as WebdriverIO.Element).$(xpath) : $(xpath)
}

export function dialog(name?: string) {
  return name ? $(`//*[@role="dialog"][.//h2[normalize-space()="${name}"]]`) : $('[role="dialog"]')
}

/** Opens the action catalog of the inspector and picks an action by its catalog id. */
export async function addAction(entry: string) {
  await button('Añadir acción').click()
  await $(`[data-entry="${entry}"]`).click()
  await $('[aria-label="Buscar acción"]').waitForExist({ reverse: true })
}

/** A silent WAV, so trying a sound key in a test makes no noise. */
export function silentWav(ms = 200, rate = 8000): Buffer {
  const samples = Math.round((rate * ms) / 1000)
  const wav = Buffer.alloc(44 + samples)
  wav.write('RIFF', 0)
  wav.writeUInt32LE(36 + samples, 4)
  wav.write('WAVEfmt ', 8)
  wav.writeUInt32LE(16, 16)
  wav.writeUInt16LE(1, 20)
  wav.writeUInt16LE(1, 22)
  wav.writeUInt32LE(rate, 24)
  wav.writeUInt32LE(rate, 28)
  wav.writeUInt16LE(1, 32)
  wav.writeUInt16LE(8, 34)
  wav.write('data', 36)
  wav.writeUInt32LE(samples, 40)
  wav.fill(128, 44)
  return wav
}

/** Opens a select and picks an option by its text. Selects react to pointer events, which the
 * WebDriver plugin does not fake, hence the full click. */
export async function chooseOption(trigger: string, option: string) {
  const select = $(`[aria-label="${trigger}"]`)
  await select.scrollIntoView({ block: 'center' })
  await clickOn(select)
  const item = $(`//*[@role="option"][normalize-space()="${option}"]`)
  await item.waitForDisplayed()
  await item.scrollIntoView({ block: 'center' })
  // The press focuses the option; its synthetic release does not pick it, Enter does.
  await clickOn(item)
  await browser.keys(Key.Enter)
  await item.waitForExist({ reverse: true })
}

/** Waits for the inspector to save what was just changed. */
export async function saved() {
  await browser.pause(100)
  await browser.waitUntil(async () => (await $('[data-save-status]').getText()).includes('Guardado'), {
    timeoutMsg: 'the key was not saved',
  })
}

/** Opens a color picker by its label, types the code and closes it. */
export async function setColor(label: string, hex: string) {
  const trigger = $(`[data-color-picker][aria-label="${label}"]`)
  await trigger.scrollIntoView({ block: 'center' })
  await clickOn(trigger)
  const code = $('[aria-label="Código del color"]')
  await code.waitForDisplayed()
  await code.setValue(hex)
  await browser.keys(Key.Enter)
  await browser.keys(Key.Escape)
  await code.waitForExist({ reverse: true })
}
