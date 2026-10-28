import { randomUUID } from 'node:crypto'
import { $, browser, expect } from '@wdio/globals'
import type { CloudStatus } from '../../../src-nuxt/app/ipc/bindings'
import { FakeCloud, SETTINGS_ID } from '../fake-cloud'
import { CLOUD_PORT, button, clickOn, dialog, engine, menuItem, shot, state } from '../helpers'

const cloud = new FakeCloud(CLOUD_PORT)
const SYNC_WAIT = 20_000
/** The app asks the cloud for other computers' changes once a minute. */
const CHECK_WAIT = 75_000

function row(name: string) {
  return $(`[data-profile="${name}"]`)
}

function mark(name: string, state: 'synced' | 'pending' | 'local') {
  return row(name).$(`[data-profile-cloud="${state}"]`)
}

async function names() {
  return (await state()).profiles.map(p => p.name)
}

async function idOf(name: string) {
  return (await state()).profiles.find(p => p.name === name)!.id
}

async function status() {
  return await engine<CloudStatus>('cloud_status')
}

function cloudName(id: string) {
  const head = cloud.head(id)
  return head?.deleted ? null : head?.content.profile.name
}

async function until(check: () => boolean | Promise<boolean>, timeoutMsg: string, timeout = SYNC_WAIT) {
  await browser.waitUntil(async () => await check(), { timeout, timeoutMsg })
}

async function openMenu(name: string) {
  await row(name).moveTo()
  await clickOn(button(`Opciones de ${name}`))
}

async function cloudTab() {
  await $('[data-sidebar-account]').click()
  await $('//*[@role="tab"][normalize-space()="Cloud"][@aria-selected="true"]').waitForExist()
}

/** Waits for the sync that `start` begins to end. */
async function synced(start: () => Promise<unknown>) {
  const before = (await status()).last_sync
  await start()
  await until(async () => {
    const now = await status()
    return !now.syncing && now.last_sync !== before
  }, 'the sync did not end')
}

async function syncNow() {
  await cloudTab()
  await synced(() => clickOn(button('Sincronizar ahora')))
}

async function syncButton() {
  await synced(() => clickOn($('[data-sidebar-sync]')))
}

/** What another computer of the account uploads: a copy of a profile it had, under another id and name. */
function profileFromElsewhere(id: string, name: string) {
  const content = [...cloud.documents.values()]
    .filter(d => d.kind === 'profile')
    .flatMap(d => d.versions)
    .find(v => v.content)!.content
  return { schema: 2, profile: { ...structuredClone(content.profile), id, name, apps: [] } }
}

describe('the cloud', () => {
  before(async () => {
    await cloud.start()
  })

  after(async () => {
    await cloud.stop()
  })

  it('offers to connect from the account in the sidebar', async () => {
    await expect($('[data-sidebar-account]')).toHaveText(expect.stringContaining('Conectar'))
    await expect($('[data-sidebar-cloud]')).not.toBeExisting()
    await expect(row('Predeterminado').$('[data-profile-cloud]')).not.toBeExisting()
    await cloudTab()
    await expect(button('Conectar')).toBeDisplayed()
    await shot('cloud-signed-out')
  })

  it('connects this computer through the browser and uploads everything', async () => {
    await clickOn(button('Conectar'))
    await until(async () => (await status()).signed_in, 'the computer did not connect')
    const id = await idOf('Predeterminado')
    await until(() => cloudName(id) === 'Predeterminado' && cloud.documents.has(SETTINGS_ID), 'nothing went up')
    const [device] = [...cloud.tokens.values()]
    expect(device!.name.length).toBeGreaterThan(0)
    await expect(mark('Predeterminado', 'synced')).toBeExisting()
    await expect($('[data-sidebar-cloud="synced"]')).toBeDisplayed()
    await expect($('[data-sidebar-sync]')).not.toBeExisting()
    await expect(button('Parar la sincronización')).toBeDisplayed()
    await shot('cloud-signed-in')
  })

  it('marks what changed here and uploads it with the sync button', async () => {
    const id = await idOf('Predeterminado')
    await engine('rename_profile', { profile: id, name: 'Editado' })
    await expect(mark('Editado', 'pending')).toBeExisting()
    await expect($('[data-sidebar-cloud="pending"]')).toBeDisplayed()
    await shot('cloud-pending')
    await syncButton()
    expect(cloudName(id)).toBe('Editado')
    await expect(mark('Editado', 'synced')).toBeExisting()
    await $('[data-sidebar-sync]').waitForExist({ reverse: true, timeout: SYNC_WAIT })
  })

  it('tells when other computers changed something and brings it', async () => {
    const id = await idOf('Editado')
    const head = cloud.head(id)!
    const now = new Date().toISOString()
    cloud.push(id, 'profile', { parent: head.id, editedAt: now, deleted: false, content: profileFromElsewhere(id, 'Desde otro equipo') })
    const other = randomUUID()
    cloud.push(other, 'profile', { parent: null, editedAt: now, deleted: false, content: profileFromElsewhere(other, 'Nuevo de otro equipo') })

    await $('[data-sidebar-sync]').waitForDisplayed({ timeout: CHECK_WAIT, timeoutMsg: 'the app did not notice the changes in the cloud' })
    await expect($('[data-sidebar-cloud="pending"]')).toBeDisplayed()
    await syncButton()
    expect(await names()).toEqual(['Desde otro equipo', 'Nuevo de otro equipo'])
    await expect(mark('Nuevo de otro equipo', 'synced')).toBeExisting()
  })

  it('deletes a synced profile here only and brings it back', async () => {
    const id = await idOf('Nuevo de otro equipo')
    await openMenu('Nuevo de otro equipo')
    await menuItem('Eliminar').click()
    const ask = dialog('¿Eliminar «Nuevo de otro equipo»?')
    await expect(ask).toBeDisplayed()
    await shot('cloud-delete')
    await button('Solo en este equipo', ask).click()
    await until(async () => !(await names()).includes('Nuevo de otro equipo'), 'the profile was not deleted here')

    await syncNow()
    expect(cloudName(id)).toBe('Nuevo de otro equipo')
    expect((await status()).elsewhere.map(p => p.name)).toEqual(['Nuevo de otro equipo'])
    await expect($('//section[.//h2[normalize-space()="En el cloud, no en este equipo"]]')).toHaveText(expect.stringContaining('Nuevo de otro equipo'))
    await clickOn(button('Traer a este equipo'))
    await until(async () => (await names()).includes('Nuevo de otro equipo'), 'the profile did not come back')
  })

  it('deletes a synced profile on every computer', async () => {
    const id = await idOf('Nuevo de otro equipo')
    await openMenu('Nuevo de otro equipo')
    await menuItem('Eliminar').click()
    await button('En todos los equipos', dialog('¿Eliminar «Nuevo de otro equipo»?')).click()
    await until(async () => !(await names()).includes('Nuevo de otro equipo'), 'the profile was not deleted')
    await $('[data-sidebar-sync]').waitForDisplayed({ timeout: SYNC_WAIT, timeoutMsg: 'the deletion was not pending' })
    await syncButton()
    expect(cloud.head(id)!.deleted).toBe(true)
    expect((await status()).elsewhere).toEqual([])
  })

  it('keeps a profile here only, with or without the copy of the others', async () => {
    const id = await idOf('Desde otro equipo')
    await openMenu('Desde otro equipo')
    await menuItem('Solo en este equipo…').click()
    const ask = dialog('¿Dejar «Desde otro equipo» solo en este equipo?')
    await expect(ask).toBeDisplayed()
    await button('Que se queden su copia', ask).click()
    await expect(mark('Desde otro equipo', 'local')).toBeExisting()
    await shot('cloud-local')
    await syncNow()
    expect(cloudName(id)).toBe('Desde otro equipo')
    expect((await status()).elsewhere).toEqual([])

    await openMenu('Desde otro equipo')
    await menuItem('Sincronizar con el cloud').click()
    await until(async () => !(await state()).profiles.find(p => p.id === id)!.local, 'the profile is still local')
    await engine('rename_profile', { profile: id, name: 'Vuelve al cloud' })
    await syncNow()
    expect(cloudName(id)).toBe('Vuelve al cloud')
    await expect(mark('Vuelve al cloud', 'synced')).toBeExisting()

    await openMenu('Vuelve al cloud')
    await menuItem('Solo en este equipo…').click()
    await button('Quitarlo de los demás', dialog('¿Dejar «Vuelve al cloud» solo en este equipo?')).click()
    await expect(mark('Vuelve al cloud', 'local')).toBeExisting()
    await syncNow()
    expect(cloud.head(id)!.deleted).toBe(true)
    expect(await names()).toContain('Vuelve al cloud')
  })

  it('syncs the passwords of the servers with the settings', async () => {
    const server = randomUUID()
    await engine('save_integration', { integration: { kind: 'obs', id: server, name: 'OBS', host: '127.0.0.1', port: 4455 } })
    await engine('obs_set_password', { server, password: 'hunter2' })
    await syncNow()
    const settings = cloud.head(SETTINGS_ID)!.content
    expect(settings.secrets[`obs:${server}`]).toBe('hunter2')
    expect(settings.integrations.map((i: { id: string }) => i.id)).toContain(server)
    expect(Object.keys(settings.secrets)).not.toContain('cloud:token')
  })

  it('says so when the account is full', async () => {
    cloud.full = true
    await engine('set_device', { settings: { brightness: 33, sleep_minutes: 0 } })
    await cloudTab()
    await clickOn(button('Sincronizar ahora'))
    await until(async () => (await status()).problem?.includes('lleno') ?? false, 'the full account was not shown')
    await expect($('[data-sidebar-cloud="problem"]')).toBeDisplayed()
    await shot('cloud-full')
    cloud.full = false
    await syncNow()
    expect((await status()).problem).toBeNull()
    await expect($('[data-sidebar-cloud="problem"]')).not.toBeExisting()
  })

  it('stops and resumes syncing', async () => {
    await cloudTab()
    await clickOn(button('Parar la sincronización'))
    await until(async () => (await status()).paused, 'the sync did not stop')
    await expect(button('Reanudar')).toBeDisplayed()
    await clickOn(button('Reanudar'))
    await until(async () => !(await status()).paused, 'the sync did not resume')
  })

  it('disconnects and, connected again, lets the user choose what to keep', async () => {
    await cloudTab()
    await clickOn(button('Desconectar'))
    await until(async () => !(await status()).signed_in, 'the computer did not disconnect')
    expect([...cloud.tokens.values()].every(t => t.revoked)).toBe(true)
    await expect($('[data-sidebar-account]')).toHaveText(expect.stringContaining('Conectar'))
    await expect($('[data-profile-cloud="synced"]')).not.toBeExisting()
    await expect(mark('Vuelve al cloud', 'local')).toBeExisting()

    const local = await idOf('Vuelve al cloud')
    const there = randomUUID()
    cloud.push(there, 'profile', { parent: null, editedAt: new Date().toISOString(), deleted: false, content: profileFromElsewhere(there, 'Solo en el cloud') })

    await clickOn(button('Conectar'))
    const join = dialog('Tu cuenta ya tiene perfiles')
    await join.waitForDisplayed({ timeout: SYNC_WAIT })
    await shot('cloud-join')
    await button('Juntar los dos', join).click()
    await until(async () => (await names()).includes('Solo en el cloud'), 'the cloud\'s profiles did not come')
    expect(await names()).toEqual(['Vuelve al cloud', 'Solo en el cloud'])
    expect(cloud.head(local)!.deleted).toBe(true)
    await expect(mark('Solo en el cloud', 'synced')).toBeExisting()
  })
})
