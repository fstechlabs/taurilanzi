import { $, $$, browser, expect } from '@wdio/globals'
import { Key } from 'webdriverio'
import type { Snapshot } from '../../../src-nuxt/app/ipc/bindings'
import {
  KEY_COUNT,
  button,
  center,
  clickAt,
  clickOn,
  dialog,
  drag,
  dragBy,
  dragPoints,
  engine,
  homePage,
  menuItem,
  node,
  pageById,
  pointOnLine,
  shot,
  state,
  text,
} from '../helpers'

const OBS = 'E2E OBS'

function keyButton(page: string, index: number) {
  return node(page).$(`button[aria-label="Tecla ${index + 1}"]`)
}

function handle(page: string, index: number) {
  return node(page).$(`.vue-flow__handle[data-handleid="key-${index}"]`)
}

/** Pulls the line from a point `at` of its length, `miss` pixels below it, by (dx, dy). */
async function pullLine(line: WebdriverIO.Element, at: number, dx: number, dy: number, miss = 0) {
  const point = await pointOnLine(line, at)
  const from = { x: point.x, y: point.y + miss }
  await dragPoints(from, { x: from.x + dx, y: from.y + dy }, 10, false, line.$('path.link-hit'))
}

async function clickLine(line: WebdriverIO.Element) {
  await clickAt(await pointOnLine(line, 0.25), { on: line.$('path.link-hit') })
}

async function settle() {
  await browser.pause(500)
}

describe('the canvas', () => {
  let home: string
  let homeName: string
  let free: number
  let existing: Set<string>
  let created: string

  const line = () => $(`.vue-flow__edge[data-id="${home}:${free}"]`)
  const nodes = () => $$('.vue-flow__node')
  const bends = async () => {
    const action = homePage(await state()).keys[free]?.actions[0]
    return action?.type === 'open_page' ? action.waypoints ?? [] : []
  }

  before(async () => {
    const snapshot = await state()
    home = snapshot.home_page
    homeName = homePage(snapshot).name
    existing = new Set(snapshot.pages.map(p => p.id))
    free = [...Array(KEY_COUNT).keys()].find(i => !homePage(snapshot).keys[i])!
    await expect(nodes()).toBeElementsArrayOfSize(existing.size)
  })

  // Only what this suite created: its pages and the key it used.
  after(async () => {
    const snapshot = await state()
    for (const page of snapshot.pages.filter(p => !existing.has(p.id))) {
      await engine('delete_page', { page: page.id })
    }
    if (homePage(await state()).keys[free]) {
      await engine('clear_key', { page: home, index: free })
    }
  })

  it('creates, moves and renames a page', async () => {
    await button('Nueva página').click()
    await expect(nodes()).toBeElementsArrayOfSize(existing.size + 1)
    await settle()
    const page = (await state()).pages.find(p => !existing.has(p.id))!
    created = page.id
    expect(page.name.startsWith('Página ')).toBe(true)
    expect(page.keys[0]?.actions).toEqual([{ type: 'back' }])

    const header = node(page.name).$('.page-drag')
    const start = await center(header)
    await dragPoints({ x: start.x - 60, y: start.y }, { x: start.x + 320, y: start.y + 60 }, 10)
    await settle()
    expect(pageById(await state(), created).position).not.toEqual(page.position)
    await button('Ver todo').click()
    await settle()

    await button(`Opciones de ${page.name}`, node(page.name)).click()
    await menuItem('Renombrar').click()
    await $('input[aria-label="Nombre de la página"]').setValue(OBS)
    await button('Guardar', dialog('Renombrar página')).click()
    await expect(node(OBS)).toBeDisplayed()
    await button('Ver todo').click()
    await settle()
    await shot('canvas-1-two-pages')
  })

  it('links a key to a page with a line', async () => {
    await drag(handle(homeName, free), node(OBS))
    await expect(line()).toBeExisting()
    expect(homePage(await state()).keys[free]?.actions).toEqual([{ type: 'open_page', page: created }])
    await shot('canvas-2-linked')

    await drag(handle(OBS, 3), node(OBS))
    await browser.pause(1000)
    await expect($(`.vue-flow__edge[data-id^="${created}:"]`)).not.toBeExisting()
  })

  it('shows the line as the key\'s first action, and sends a page to the dock', async () => {
    await keyButton(OBS, 0).click()
    await expect(text(`Tecla 1 · ${OBS}`)).toBeDisplayed()
    await expect($('[aria-label="Acción 1: Volver"]')).toBeDisplayed()

    await keyButton(homeName, free).click()
    await expect($('[aria-label="Acción 1: Ir a página"]')).toBeDisplayed()
    await expect($('[aria-label="Página de destino"]')).toHaveText(expect.stringContaining('OBS'))

    await button(`Opciones de ${OBS}`, node(OBS)).click()
    await menuItem('Mostrar en el dock').click()
    await expect(text('En el dock', node(OBS))).toBeDisplayed()
    expect((await state()).path).toEqual([home, created])
  })

  it('bends lines: pulling makes a point, points move and a double click deletes them', async () => {
    const pane = await $('.vue-flow__pane').getLocation()
    await clickAt({ x: Math.round(pane.x + 30), y: Math.round(pane.y + 400) })
    await clickLine(await line().getElement())
    await expect(button('Eliminar tecla y enlace')).toBeDisplayed()
    // The open inspector narrows the canvas: the home page may have fallen out of view.
    await button('Ver todo').click()
    await settle()

    await pullLine(await line().getElement(), 0.25, 0, 140, 9)
    await settle()
    const first = await bends()
    expect(first).toHaveLength(1)
    await expect($('[data-bend="0"]')).toBeDisplayed()
    await dragBy($('[data-bend="0"]'), 60, 0)
    await settle()
    const moved = await bends()
    expect(moved).toHaveLength(1)
    expect(moved[0]!.x).toBeGreaterThan(first[0]!.x + 30)
    await shot('canvas-4-bent-line')
    await clickOn($('[data-bend="0"]'), { double: true })
    await settle()
    expect(await bends()).toEqual([])
    await pullLine(await line().getElement(), 0.3, 0, 120)
    await settle()
    await pullLine(await line().getElement(), 0.8, 0, -100)
    await settle()
    expect(await bends()).toHaveLength(2)
  })

  it('removes a line together with its key', async () => {
    await clickLine(await line().getElement())
    await button('Eliminar tecla y enlace').click()
    await expect(line()).not.toBeExisting()
    expect(homePage(await state()).keys[free]).toBeUndefined()
  })

  it('moves keys within a page and swaps them across pages', async () => {
    await drag(keyButton(OBS, 0), keyButton(OBS, 6), true)
    await settle()
    let keys = pageById(await state(), created).keys
    expect(Object.keys(keys)).toEqual(['6'])
    expect(keys[6]?.actions).toEqual([{ type: 'back' }])

    await drag(handle(homeName, free), node(OBS))
    await expect(line()).toBeExisting()
    await drag(keyButton(homeName, free), keyButton(OBS, 6), true)
    await expect(line()).not.toBeExisting()
    const snapshot: Snapshot = await state()
    keys = pageById(snapshot, created).keys
    expect(keys[6]?.actions).toEqual([])
    expect(homePage(snapshot).keys[free]?.actions).toEqual([{ type: 'back' }])
    await shot('canvas-3-moved')
  })

  it('deletes the selected line and its key with Delete', async () => {
    await engine('clear_key', { page: home, index: free })
    await drag(handle(homeName, free), node(OBS))
    await expect(line()).toBeExisting()
    await clickLine(await line().getElement())
    await browser.keys(Key.Delete)
    await expect(line()).not.toBeExisting()
    expect(homePage(await state()).keys[free]).toBeUndefined()
  })

  it('opens the app\'s own menus on right click', async () => {
    const prevented = await browser.execute(() => {
      const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
      document.querySelector('aside')!.dispatchEvent(event)
      return event.defaultPrevented
    })
    expect(prevented).toBe(true)

    await drag(handle(homeName, free), node(OBS))
    await expect(line()).toBeExisting()
    await clickOn(keyButton(homeName, free), { button: 'right' })
    await menuItem('Eliminar tecla').click()
    await expect(line()).not.toBeExisting()
    expect(homePage(await state()).keys[free]).toBeUndefined()

    const before = (await state()).pages.length
    const pane = await $('.vue-flow__pane').getLocation()
    await clickAt({ x: Math.round(pane.x + 40), y: Math.round(pane.y + 100) }, { button: 'right' })
    await menuItem('Nueva página aquí').click()
    await expect(nodes()).toBeElementsArrayOfSize(before + 1)
    const extra = (await state()).pages.find(p => !existing.has(p.id) && p.id !== created)!
    await engine('delete_page', { page: extra.id })
    await expect(nodes()).toBeElementsArrayOfSize(before)

    await clickOn(node(OBS).$('.page-drag'), { button: 'right' })
    await expect(menuItem('Renombrar')).toBeDisplayed()
    await browser.keys(Key.Escape)
    await expect($('[role="menu"]')).not.toBeExisting()
  })

  it('deletes a page and the lines into it', async () => {
    await drag(handle(homeName, free), node(OBS))
    await expect(line()).toBeExisting()
    await button(`Opciones de ${OBS}`, node(OBS)).click()
    await menuItem('Eliminar').click()
    await expect(nodes()).toBeElementsArrayOfSize(existing.size)
    await expect(line()).not.toBeExisting()
    const snapshot = await state()
    expect(homePage(snapshot).keys[free]?.actions).toEqual([])
    expect(snapshot.path).toEqual([home])

    await button('Deshacer').click()
    await expect(node(OBS)).toBeDisplayed()
    await expect(line()).toBeExisting()
    expect(homePage(await state()).keys[free]?.actions).toEqual([{ type: 'open_page', page: created }])
    await button('Rehacer').click()
    await expect(node(OBS)).not.toBeExisting()
  })
})
