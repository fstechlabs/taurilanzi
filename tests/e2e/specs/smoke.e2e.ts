import { $, $$, browser, expect } from '@wdio/globals'
import { dragBy, homePage, shot, state } from '../helpers'

describe('the app', () => {
  it('shows the home page on the canvas and selects a key', async () => {
    await expect($('.vue-flow__node')).toBeExisting()
    const snapshot = await state()
    expect(snapshot.pages).toHaveLength(1)
    const home = homePage(snapshot)
    await expect($(`[data-page="${home.name}"]`)).toBeDisplayed()

    await $('button[aria-label="Tecla 1"]').click()
    await expect($(`//*[normalize-space()="Tecla 1 · ${home.name}"]`)).toBeDisplayed()
    await expect($$('img[src^="deck://localhost/key/"]')).toBeElementsArrayOfSize({ gte: 1 })
    await shot('smoke')

    // Relative: the width is remembered in storage the app the user runs shares with this one.
    const inspector = $('aside:has([data-inspector-resize])')
    const before = (await inspector.getSize()).width
    await dragBy($('[data-inspector-resize]'), -120, 0, true)
    expect((await inspector.getSize()).width).toBe(Math.min(900, before + 120))
    await dragBy($('[data-inspector-resize]'), 120, 0, true)
    expect((await inspector.getSize()).width).toBe(before)
    await browser.pause(200)
  })
})
