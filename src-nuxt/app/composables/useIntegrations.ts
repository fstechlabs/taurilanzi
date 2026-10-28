import { commands } from '~/ipc/bindings'
import { hassStatusText } from '~/lib/actions/hass'
import { obsStatusText } from '~/lib/actions/obs'
import type { Integration } from '~/types/engine'

export type IntegrationKind = Integration['kind']

export { NO_SERVER } from '~/lib/integrations'

/** The servers of OBS and Home Assistant, and how each is doing. */
export function useIntegrations() {
  const { snapshot } = useEngine()
  const all = computed(() => snapshot.value?.integrations ?? [])

  function servers(kind: IntegrationKind) {
    return all.value.filter(server => server.kind === kind)
  }

  function server(id: string | null | undefined) {
    return all.value.find(server => server.id === id)
  }

  type State = 'off' | 'connecting' | 'connected' | 'failed'

  /** How a server is doing: its state, in a word for its dot, and in a sentence. */
  function status(id: string): { state: State, text: string, detail: string | null, secretSet: boolean } {
    const found = server(id)
    if (found?.kind === 'obs') {
      const info = snapshot.value?.obs[id]
      return {
        state: info?.status.state ?? 'off',
        text: info ? obsStatusText(info) : '',
        detail: info?.status.state === 'failed' ? info.status.detail : null,
        secretSet: Boolean(info?.password_set),
      }
    }
    const info = snapshot.value?.hass[id]
    return {
      state: info?.status.state ?? 'off',
      text: info ? hassStatusText(info) : '',
      detail: info?.status.state === 'failed' ? info.status.detail : null,
      secretSet: Boolean(info?.token_set),
    }
  }

  async function save(integration: Integration) {
    await call(commands.saveIntegration(integration))
  }

  async function remove(id: string) {
    await call(commands.deleteIntegration(id))
  }

  /** Saves the password of an OBS or the token of a Home Assistant with the secrets; null removes it. */
  async function saveSecret(integration: Integration, secret: string | null) {
    if (integration.kind === 'obs') {
      await call(commands.obsSetPassword(integration.id, secret))
    }
    else {
      await call(commands.hassSetToken(integration.id, secret))
    }
  }

  function connect(integration: Integration) {
    if (integration.kind === 'obs') {
      commands.obsConnect(integration.id)
    }
    else {
      commands.hassConnect(integration.id)
    }
  }

  return { all, servers, server, status, save, remove, saveSecret, connect }
}
