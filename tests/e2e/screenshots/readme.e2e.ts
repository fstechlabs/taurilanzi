import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { crc32, deflateSync, inflateSync } from 'node:zlib'
import { $, browser, expect } from '@wdio/globals'
import type { Action, Color, Key, LayerConfig, Preferences, Widget } from '../../../src-nuxt/app/ipc/bindings'
import { button, clickOn, engine, homePage, node, state } from '../helpers'

const OUT = join(import.meta.dirname, '..', '..', '..', 'docs', 'screenshots')
const WIDE = 13

const rgb = (hex: string): Color => ({
  r: Number.parseInt(hex.slice(1, 3), 16),
  g: Number.parseInt(hex.slice(3, 5), 16),
  b: Number.parseInt(hex.slice(5, 7), 16),
})
const WHITE = rgb('#ffffff')

/**
 * Keeps the first `height` rows of a PNG. Each row is filtered against the one above it only,
 * so the rows kept need no re-encoding.
 */
function cropRows(png: Buffer, height: number): Buffer {
  const chunks: { type: string, data: Buffer }[] = []
  for (let at = 8; at < png.length;) {
    const length = png.readUInt32BE(at)
    chunks.push({ type: png.toString('latin1', at + 4, at + 8), data: png.subarray(at + 8, at + 8 + length) })
    at += 12 + length
  }
  const header = Buffer.from(chunks.find(c => c.type === 'IHDR')!.data)
  const width = header.readUInt32BE(0)
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[header[9]!]!
  if (header[8] !== 8 || header[12] !== 0) {
    throw new Error('only 8-bit, non-interlaced PNGs')
  }
  header.writeUInt32BE(height, 4)
  const rows = inflateSync(Buffer.concat(chunks.filter(c => c.type === 'IDAT').map(c => c.data)))
  const pixels = deflateSync(rows.subarray(0, height * (1 + width * channels)))
  const chunk = (type: string, data: Buffer) => {
    const head = Buffer.alloc(8)
    head.writeUInt32BE(data.length, 0)
    head.write(type, 4, 'latin1')
    const crc = Buffer.alloc(4)
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0)
    return Buffer.concat([head, data, crc])
  }
  const out = [png.subarray(0, 8)]
  let written = false
  for (const c of chunks) {
    if (c.type === 'IHDR') {
      out.push(chunk('IHDR', header))
    }
    else if (c.type !== 'IDAT') {
      out.push(chunk(c.type, c.data))
    }
    else if (!written) {
      out.push(chunk('IDAT', pixels))
      written = true
    }
  }
  return Buffer.concat(out)
}

/** The screenshot covers the whole window, title bar included, which leaves a band below the page. */
async function save(name: string) {
  mkdirSync(OUT, { recursive: true })
  await browser.pause(800)
  const height = await browser.execute(() => Math.round(document.documentElement.clientHeight * devicePixelRatio))
  const png = Buffer.from(await browser.takeScreenshot(), 'base64')
  writeFileSync(join(OUT, `${name}.png`), cropRows(png, height))
}

async function icon(name: string): Promise<LayerConfig> {
  const response = await fetch(`https://api.iconify.design/${name.replace(':', '/')}.svg?color=white`)
  const svg = new Uint8Array(await response.arrayBuffer())
  const asset = await browser.execute(
    (bytes: number[]) => (window as any).__TAURI_INTERNALS__.invoke('put_asset', new Uint8Array(bytes), {
      headers: { 'Asset-Type': 'image/svg+xml' },
    }),
    [...svg],
  ) as string
  return { type: 'image', asset, fit: 'contain', inset: 0.26, icon: name }
}

function label(content: string, position: 'top' | 'middle' | 'bottom' = 'bottom', size = 26): LayerConfig {
  return { type: 'text', content, font_family: 'Inter', font_weight: 700, size, color: WHITE, position, outline: null }
}

function key(background: string, layers: LayerConfig[], actions: Action[] = []): Key {
  return {
    appearance: { background: { type: 'solid', color: rgb(background) }, layers },
    actions,
    double_press: [],
    long_press: [],
    second_state: null,
  }
}

function wide(widget: Widget, from: string, to: string): Key {
  return {
    appearance: {
      background: { type: 'linear_gradient', from: rgb(from), to: rgb(to), angle: 0 },
      layers: [{ type: 'widget', widget }],
    },
    actions: [],
    double_press: [],
    long_press: [],
    second_state: null,
  }
}

const style = (accent: string) => ({ color: WHITE, accent: rgb(accent), font_family: 'Inter' })

async function setKeys(page: string, keys: Record<number, Key>) {
  for (const [index, k] of Object.entries(keys)) {
    await engine('set_key', { page, index: Number(index), key: k })
  }
}

/** A demo setup in English, so the README shows the app as someone would use it. */
describe('screenshots for the README', () => {
  before(async () => {
    const preferences: Preferences = { ...(await state()).preferences, language: 'en' }
    await engine('set_preferences', { preferences })
    await browser.waitUntil(async () => (await state()).language === 'en')
    await browser.setWindowSize(1360, 860)

    const snapshot = await state()
    const home = snapshot.home_page
    await engine('rename_profile', { profile: snapshot.active_profile, name: 'Everyday' })
    await engine('update_page', { page: home, changes: { name: 'Home', position: { x: 0, y: 0 } } })
    await engine('create_profile', { name: 'Streaming' })
    await engine('create_profile', { name: 'Video editing' })

    const obs = await engine<string>('create_page', { name: 'OBS', position: { x: 460, y: -150 } })
    const media = await engine<string>('create_page', { name: 'Media', position: { x: 460, y: 150 } })

    await setKeys(home, {
      0: key('#24292f', [await icon('simple-icons:github'), label('GitHub')], [{ type: 'open_url', url: 'https://github.com' }]),
      1: key('#b7410e', [await icon('simple-icons:rust'), label('Rust')], [{ type: 'open_url', url: 'https://doc.rust-lang.org' }]),
      2: key('#24c8db', [await icon('simple-icons:tauri'), label('Tauri')], [{ type: 'open_url', url: 'https://v2.tauri.app' }]),
      3: key('#008c5a', [await icon('simple-icons:nuxtdotjs'), label('Nuxt')], [{ type: 'open_url', url: 'https://nuxt.com' }]),
      4: key('#3a3a3c', [await icon('mdi:broadcast'), label('OBS')], [{ type: 'open_page', page: obs }]),
      5: key('#5e5ce6', [await icon('mdi:slack'), label('Slack')]),
      6: key('#ff375f', [await icon('mdi:calendar'), label('Calendar')]),
      7: key('#ff9f0a', [await icon('mdi:lightbulb-on'), label('Lights')]),
      8: key('#30d158', [await icon('mdi:console'), label('Terminal')]),
      9: key('#3a3a3c', [await icon('mdi:music'), label('Media')], [{ type: 'open_page', page: media }]),
      10: key('#1c1c1e', [await icon('mdi:lock'), label('Lock')]),
      11: key('#1c1c1e', [await icon('mdi:content-copy'), label('Copy')]),
      12: key('#1c1c1e', [await icon('mdi:content-paste'), label('Paste')]),
      [WIDE]: wide({ kind: 'digital_clock', seconds: false, hour12: false, second_line: 'weekday_date', zone: null, label: '', style: style('#ff9f0a') }, '#141828', '#3c1450'),
    })

    await setKeys(obs, {
      1: key('#ff3b30', [await icon('mdi:record-rec'), label('Record')]),
      2: key('#bf5af2', [await icon('mdi:access-point'), label('Go live')]),
      3: key('#3a3a3c', [await icon('mdi:microphone-off'), label('Mic')]),
      5: key('#2c2c2e', [label('Camera', 'middle', 30)]),
      6: key('#2c2c2e', [label('Screen', 'middle', 30)]),
      7: key('#2c2c2e', [label('Break', 'middle', 30)]),
      [WIDE]: wide({ kind: 'system_monitor', metrics: ['cpu', 'ram', 'gpu'], style: style('#30d158') }, '#101418', '#14281e'),
    })

    await setKeys(media, {
      1: key('#1c1c1e', [await icon('mdi:skip-previous'), label('Previous')], [{ type: 'media', key: 'previous' }]),
      2: key('#1c1c1e', [await icon('mdi:play-pause'), label('Play')], [{ type: 'media', key: 'play_pause' }]),
      3: key('#1c1c1e', [await icon('mdi:skip-next'), label('Next')], [{ type: 'media', key: 'next' }]),
      6: key('#1c1c1e', [await icon('mdi:volume-minus'), label('Volume')], [{ type: 'media', key: 'volume_down' }]),
      7: key('#1c1c1e', [await icon('mdi:volume-plus'), label('Volume')], [{ type: 'media', key: 'volume_up' }]),
      8: key('#1c1c1e', [await icon('mdi:volume-off'), label('Mute')], [{ type: 'media', key: 'mute' }]),
      [WIDE]: wide({ kind: 'calendar', style: style('#ff375f') }, '#1a1020', '#40142a'),
    })
  })

  it('the canvas with a key open in the editor', async () => {
    await button('Show all').click()
    const home = homePage(await state())
    await clickOn(node(home.name).$('button[aria-label="Key 1"]'))
    await expect($('[aria-label="Action 1: Open website"]')).toBeExisting()
    await save('editor')
  })

  it('live content on the wide key', async () => {
    const home = homePage(await state())
    await clickOn(node(home.name).$(`button[aria-label="Key ${WIDE + 1}"]`))
    await save('wide-key')
  })

  it('the actions a key can run', async () => {
    await clickOn(node('Media').$('button[aria-label="Key 10"]'))
    await button('Set up key').click()
    await button('Add action').click()
    await save('actions')
    await browser.keys('Escape')
  })

  it('the settings', async () => {
    await $('a[href="/settings"]').click()
    await expect($('h1=Settings')).toBeDisplayed()
    await clickOn($('//*[@role="tab"][normalize-space()="Lock and idle"]'))
    await save('settings')
  })
})
