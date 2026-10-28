import { commands } from '~/ipc/bindings'
import type { AppRef } from '~/types/engine'
import { t } from '~/lib/i18n'

/** Windows serves custom schemes over http. */
const IMAGES = import.meta.client && navigator.userAgent.includes('Windows') ? 'http://deck.localhost' : 'deck://localhost'

/** Profiles, the apps linked to them, and switching between them. Edits can be undone. */
export function useProfiles() {
  const { snapshot } = useEngine()

  const profiles = computed(() => snapshot.value?.profiles ?? [])
  const active = computed(() => snapshot.value?.active_profile ?? null)
  const frontApp = computed(() => snapshot.value?.front_app ?? null)
  /** The app in front that put the active profile on the dock. */
  const chosenBy = computed(() => (snapshot.value?.automatic_profile ? frontApp.value : null))
  const followApps = computed(() => snapshot.value?.preferences.profiles?.follow_apps ?? true)

  function profile(id: string) {
    return profiles.value.find(p => p.id === id)
  }

  /** The profile an app is linked to, if any. */
  function owner(appId: string) {
    return profiles.value.find(p => p.apps.some(a => a.id === appId))
  }

  async function activate(id: string) {
    await call(commands.setProfile(id))
  }

  /** New and copied profiles go on the dock, so they can be edited straight away. */
  async function create(name?: string) {
    const id = await call(commands.createProfile(name?.trim() || null))
    await activate(id)
    return id
  }

  async function rename(id: string, name: string) {
    await call(commands.renameProfile(id, name.trim()))
  }

  async function duplicate(id: string) {
    const copy = await call(commands.duplicateProfile(id))
    await activate(copy)
    return copy
  }

  /** A synced profile goes from every computer and the cloud, or, without `everywhere`, from this one only. */
  async function remove(id: string, everywhere = true) {
    await call(commands.deleteProfile(id, everywhere))
  }

  /** Kept here only, the other computers keep their copy with `keepInCloud` and lose it without. */
  async function setLocal(id: string, local: boolean, keepInCloud = false) {
    await call(commands.setProfileLocal(id, local, keepInCloud))
  }

  async function moveTo(id: string, index: number) {
    await call(commands.moveProfile(id, index))
  }

  /** Throws a message for the user when the app already belongs to another profile. */
  async function link(profileId: string, app: AppRef) {
    const other = owner(app.id)
    if (other && other.id !== profileId) {
      throw new Error(t('shell.linkedApps.alreadyLinked', { app: app.name, profile: other.name }))
    }
    try {
      await call(commands.linkApp(profileId, app))
    }
    catch (err) {
      if (err instanceof EngineError && err.kind === 'conflict') {
        throw new Error(t('shell.linkedApps.linkedElsewhere', { app: app.name }))
      }
      throw new Error(t('shell.linkedApps.linkFailed', { app: app.name }))
    }
  }

  async function unlink(profileId: string, appId: string) {
    await call(commands.unlinkApp(profileId, appId))
  }

  async function appAt(path: string) {
    return await call(commands.appAtPath(path))
  }

  async function runningApps() {
    return await commands.runningApps()
  }

  /** Whether the desktop lets the app know which app is in front. */
  async function appDetection() {
    return await commands.appDetection()
  }

  async function setUpAppDetection() {
    return await call(commands.setUpAppDetection())
  }

  function iconUrl(appId: string) {
    return `${IMAGES}/app-icon/${encodeURIComponent(appId)}`
  }

  return {
    profiles,
    active,
    frontApp,
    chosenBy,
    followApps,
    profile,
    owner,
    activate,
    create,
    rename,
    duplicate,
    remove,
    setLocal,
    moveTo,
    link,
    unlink,
    appAt,
    runningApps,
    appDetection,
    setUpAppDetection,
    iconUrl,
  }
}
