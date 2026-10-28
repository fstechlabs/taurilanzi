import { commands, events, type CloudStatus, type JoinMode } from '~/ipc/bindings'

const status = ref<CloudStatus | null>(null)
let started = false

/** This computer and the cloud: signing in, syncing and what it is doing, kept up to date by the engine. */
export function useCloud() {
  if (!started && import.meta.client) {
    started = true
    events.cloudChanged.listen(({ payload }) => {
      status.value = payload
    })
    commands.cloudStatus().then((value) => {
      status.value = value
    })
  }

  const signedIn = computed(() => status.value?.signed_in ?? false)

  /** Resolves once the browser comes back; rejects if the user cancels or the account says no. */
  async function signIn() {
    await call(commands.cloudSignIn())
  }

  async function signOut() {
    await call(commands.cloudSignOut())
  }

  async function setPaused(paused: boolean) {
    await call(commands.cloudSetPaused(paused))
  }

  async function openWeb() {
    await call(commands.cloudOpenWeb())
  }

  async function bringBack(id: string) {
    await call(commands.cloudBringBack(id))
  }

  return {
    status: readonly(status),
    signedIn,
    signIn,
    cancelSignIn: () => commands.cloudCancelSignIn(),
    signOut,
    syncNow: () => commands.cloudSyncNow(),
    setPaused,
    join: (mode: JoinMode) => commands.cloudJoin(mode),
    openWeb,
    bringBack,
  }
}
