import { commands, events, type McpStatus, type PluginRecommended } from '~/ipc/bindings'

export type McpState = 'off' | 'starting' | 'running' | 'error'

const status = ref<McpStatus | null>(null)
/** A store plugin an assistant suggested, until the store shows it. */
const recommendation = ref<PluginRecommended | null>(null)
let started = false

async function settle(pending: Promise<{ status: 'ok', data: McpStatus } | { status: 'error', error: null }>) {
  const result = await pending
  if (result.status === 'ok') {
    status.value = result.data
  }
}

/** The local MCP server that lets AI assistants configure the dock, kept up to date by the engine. */
export function useMcp() {
  if (!started && import.meta.client) {
    started = true
    events.mcpChanged.listen(({ payload }) => {
      status.value = payload
    })
    events.pluginRecommended.listen(({ payload }) => {
      recommendation.value = payload
    })
    settle(commands.mcpStatus())
  }

  const state = computed<McpState>(() => {
    const value = status.value
    if (!value?.enabled) {
      return 'off'
    }
    if (value.error) {
      return 'error'
    }
    return value.running ? 'running' : 'starting'
  })

  return {
    status: readonly(status),
    state,
    recommendation,
    setEnabled: (enabled: boolean) => settle(commands.setMcpEnabled(enabled)),
    regenerateToken: () => settle(commands.regenerateMcpToken()),
  }
}
