// Runs with a virtual D200 connected (run.ts): the first dock stays unplugged next to it.
import { $, browser, expect } from '@wdio/globals'
import { button, clickOn, dialog, menuItem, shot, state } from '../helpers'

const FIRST = 'Ulanzi D200'
const VIRTUAL = 'Ulanzi D200 (VIRTUAL-1)'

function dock(name: string) {
  return $(`[data-dock="${name}"]`)
}

async function until(check: () => Promise<boolean>, timeoutMsg: string) {
  await browser.waitUntil(check, { timeoutMsg })
}

async function device(name: string) {
  return (await state()).devices.find(d => d.name === name)!
}

async function profileNames() {
  return (await state()).profiles.map(p => p.name)
}

async function transfer(profile: string) {
  await clickOn(button(`Opciones de ${profile}`))
  await menuItem('Mover o copiar a otro dock…').click()
  const open = $('[data-profile-transfer]')
  await expect(open).toBeDisplayed()
  await expect(open.$('[data-transfer-fits]')).toBeDisplayed()
  return open
}

describe('several docks', () => {
  it('lists every dock it knows, connected or not', async () => {
    await until(async () => (await state()).devices.length === 2, 'the virtual dock did not connect')
    await expect(dock(FIRST).$('[data-dock-connected="false"]')).toBeExisting()
    await expect(dock(VIRTUAL).$('[data-dock-connected="true"]')).toBeExisting()
    await expect($('[data-dock-untested]')).not.toBeExisting()
  })

  it('shows the profiles of the dock chosen in the sidebar', async () => {
    const first = await device(FIRST)
    const other = await device(VIRTUAL)
    expect((await state()).device_id).toBe(first.id)
    await clickOn(dock(VIRTUAL).$(`span[title="${VIRTUAL}"]`))
    await until(async () => (await state()).device_id === other.id, 'the other dock was not shown')
    const snapshot = await state()
    expect(snapshot.profiles.map(p => p.id)).toEqual([other.active_profile])
    expect(other.active_profile).not.toBe(first.active_profile)
    await shot('devices-two-docks')
  })

  it('renames a dock, showing its model and serial number', async () => {
    await clickOn(button(`Opciones de ${VIRTUAL}`))
    await menuItem('Renombrar').click()
    const form = dialog('Renombrar el dock')
    await expect(form.$('[data-dock-identity]')).toHaveText(expect.stringContaining('VIRTUAL-1'))
    await form.$('[aria-label="Nombre del dock"]').setValue('Mesa')
    await button('Guardar', form).click()
    await until(async () => (await state()).devices.some(d => d.name === 'Mesa'), 'the dock was not renamed')
    await expect(dock('Mesa')).toBeDisplayed()
    await expect(dock(FIRST)).toBeDisplayed()
  })

  it('copies the only profile of a connected dock, which cannot move', async () => {
    const open = await transfer('Predeterminado')
    await expect(open.$('[data-transfer-only]')).toBeDisplayed()
    await expect(open.$('[data-transfer-move]')).toBeDisabled()
    await open.$('[data-transfer-copy]').click()
    await open.waitForExist({ reverse: true })
    const first = await device(FIRST)
    await until(async () => (await state()).device_id === first.id, 'the editor did not follow the copy')
    expect(await profileNames()).toEqual(['Predeterminado', 'Predeterminado (copia)'])
    expect((await device('Mesa')).profiles).toBe(1)
  })

  it('moves a profile to another dock and undoes it', async () => {
    const open = await transfer('Predeterminado (copia)')
    await expect(open.$('[data-transfer-only]')).not.toBeExisting()
    await open.$('[data-transfer-move]').click()
    await open.waitForExist({ reverse: true })
    await until(async () => (await device('Mesa')).profiles === 2, 'the profile did not move')
    expect((await device(FIRST)).profiles).toBe(1)
    expect(await profileNames()).toEqual(['Predeterminado', 'Predeterminado (copia)'])

    await button('Deshacer').click()
    await until(async () => (await device(FIRST)).profiles === 2, 'undo did not bring it back')
    expect((await device('Mesa')).profiles).toBe(1)
  })

  it('forgets an unplugged dock with its profiles, never a connected one', async () => {
    await clickOn(button('Opciones de Mesa'))
    await expect(menuItem('Olvidar este dock…')).toBeDisabled()
    await browser.keys('Escape')

    await clickOn(button(`Opciones de ${FIRST}`))
    await menuItem('Olvidar este dock…').click()
    const confirm = $('[data-dock-forget]')
    await expect(confirm).toBeDisplayed()
    await button('Olvidar el dock', confirm).click()
    await until(async () => (await state()).devices.length === 1, 'the dock was not forgotten')
    const snapshot = await state()
    expect(snapshot.devices[0]!.name).toBe('Mesa')
    expect(snapshot.device_id).toBe(snapshot.devices[0]!.id)
    expect(snapshot.profiles.map(p => p.name)).toEqual(['Predeterminado'])
    await expect(dock(FIRST)).not.toBeExisting()
  })
})
