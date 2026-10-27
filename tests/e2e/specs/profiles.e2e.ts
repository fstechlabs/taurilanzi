import { $, browser, expect } from '@wdio/globals'
import {
  button,
  center,
  clickOn,
  dialog,
  dragPoints,
  engine,
  menuItem,
  node,
  shot,
  state,
} from '../helpers'

const FINDER = { id: 'com.apple.finder', name: 'Finder' }

function row(name: string) {
  return $(`[data-profile="${name}"]`)
}

async function names() {
  return (await state()).profiles.map(p => p.name)
}

async function profileNamed(name: string) {
  return (await state()).profiles.find(p => p.name === name)!
}

async function until(check: () => Promise<boolean>, timeoutMsg: string) {
  await browser.waitUntil(check, { timeoutMsg })
}

async function openMenu(name: string) {
  await clickOn(button(`Opciones de ${name}`))
}

async function nameIt(title: string, value: string, confirm: string) {
  const form = dialog(title)
  await expect(form).toBeDisplayed()
  await form.$('[aria-label="Nombre del perfil"]').setValue(value)
  await button(confirm, form).click()
  await form.waitForExist({ reverse: true })
}

describe('the profiles', () => {
  it('lists the profiles in the sidebar without their pages', async () => {
    await expect(row('Predeterminado')).toBeDisplayed()
    await expect(row('Predeterminado')).toHaveAttribute('aria-current', 'true')
    await expect($('[title^="Ver "][title$=" en la mesa"]')).not.toBeExisting()
  })

  it('creates a profile with an empty home page and switches to it', async () => {
    await button('Nuevo perfil').click()
    await nameIt('Nuevo perfil', 'Trabajo', 'Crear')
    await until(async () => (await names()).includes('Trabajo'), 'the profile was not created')
    const snapshot = await state()
    const created = snapshot.profiles.find(p => p.name === 'Trabajo')!
    expect(snapshot.active_profile).toBe(created.id)
    expect(snapshot.pages.map(p => p.name)).toEqual(['Inicio'])
    expect(Object.keys(snapshot.pages[0]!.keys)).toEqual([])
    await expect(row('Trabajo')).toHaveAttribute('aria-current', 'true')
    await expect(node('Inicio')).toBeDisplayed()
  })

  it('renames and duplicates a profile', async () => {
    await openMenu('Trabajo')
    await menuItem('Renombrar').click()
    await nameIt('Renombrar perfil', 'Oficina', 'Guardar')
    await until(async () => (await names()).includes('Oficina'), 'the profile was not renamed')

    await openMenu('Oficina')
    await menuItem('Duplicar').click()
    await until(async () => (await names()).includes('Oficina (copia)'), 'the profile was not duplicated')
    expect(await names()).toEqual(['Predeterminado', 'Oficina', 'Oficina (copia)'])
  })

  it('links apps, shows their icons and keeps an app in one profile', async () => {
    const office = await profileNamed('Oficina')
    await openMenu('Oficina')
    await menuItem('Apps vinculadas…').click()
    const apps = dialog('Apps de «Oficina»')
    await expect(apps).toBeDisplayed()
    const finder = apps.$('//*[@aria-label="Apps abiertas"]//button[.//span[normalize-space()="Finder"]]')
    await finder.waitForDisplayed()
    await finder.click()
    await until(async () => (await profileNamed('Oficina')).apps.some(a => a.id === FINDER.id), 'Finder was not linked')
    await expect(apps.$('aria/Quitar Finder')).toBeDisplayed()
    await shot('profiles-linked-apps')
    await browser.keys('Escape')
    await apps.waitForExist({ reverse: true })
    await expect(row('Oficina').$(`img[src*="app-icon/${FINDER.id}"]`)).toBeDisplayed()

    const copy = await profileNamed('Oficina (copia)')
    const refused = await browser.execute(async (profile, app) => {
      try {
        await (window as any).__TAURI_INTERNALS__.invoke('link_app', { profile, app })
        return null
      }
      catch (err) {
        return String((err as { message?: string }).message ?? err)
      }
    }, copy.id, FINDER)
    expect(refused).toContain('Finder')
    expect((await profileNamed('Oficina (copia)')).apps).toEqual([])

    await engine('unlink_app', { profile: office.id, app: FINDER.id })
    await until(async () => (await profileNamed('Oficina')).apps.length === 0, 'Finder was not unlinked')
  })

  it('reorders the profiles by dragging them', async () => {
    const from = await center(row('Predeterminado'))
    const last = await center(row('Oficina (copia)'))
    await dragPoints(from, { x: from.x, y: last.y + 12 })
    await until(async () => (await names()).at(-1) === 'Predeterminado', 'the profile did not move')
    expect(await names()).toEqual(['Oficina', 'Oficina (copia)', 'Predeterminado'])
    await shot('profiles-reordered')
  })

  it('deletes a profile at once and brings it back with undo', async () => {
    await openMenu('Oficina (copia)')
    await menuItem('Eliminar').click()
    await expect(dialog()).not.toBeExisting()
    await until(async () => !(await names()).includes('Oficina (copia)'), 'the profile was not deleted')
    await button('Deshacer').click()
    await until(async () => (await names()).includes('Oficina (copia)'), 'undo did not bring it back')
  })

  it('never deletes the last profile', async () => {
    for (const name of ['Oficina (copia)', 'Oficina']) {
      await engine('delete_profile', { profile: (await profileNamed(name)).id })
    }
    await until(async () => (await names()).length === 1, 'the profiles were not deleted')
    await openMenu('Predeterminado')
    await expect(menuItem('Eliminar')).toBeDisabled()
    await browser.keys('Escape')
  })

  it('turns off switching by the app in front from the settings', async () => {
    await $('a[href="/settings"]').click()
    await clickOn($('//*[@role="tab"][normalize-space()="Perfiles"]'))
    const toggle = $('[aria-label="Cambiar de perfil según la app activa"]')
    await expect(toggle).toHaveAttribute('aria-checked', 'true')
    await toggle.click()
    await until(async () => !(await state()).preferences.profiles!.follow_apps, 'the setting was not saved')
    await clickOn(row('Predeterminado'))
    await expect(node('Inicio')).toBeDisplayed()
  })
})
