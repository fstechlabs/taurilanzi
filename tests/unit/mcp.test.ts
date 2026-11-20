import { expect, test } from 'bun:test'
import { t } from '../../src-nuxt/app/lib/i18n'
import { ASSISTANTS, clientName, snippets, timeAgo, tokenVariable } from '../../src-nuxt/app/lib/mcp'

const connection = {
  url: 'http://127.0.0.1:47833/mcp',
  token: 'secret-token',
  bridge: 'C:\\Program Files\\App\\app.exe',
}

test('every assistant gets its snippets with the live address and token', () => {
  for (const assistant of ASSISTANTS) {
    const blocks = snippets(assistant, connection, 'macos')
    expect(blocks.length).toBeGreaterThan(0)
    expect(blocks.map(b => b.code).join('\n')).toContain('secret-token')
  }
})

test('Claude Code and Gemini CLI add the server with a command', () => {
  expect(snippets('claude-code', connection, 'macos')[0]!.code).toBe(
    'claude mcp add --transport http --scope user taurilanzi http://127.0.0.1:47833/mcp --header "Authorization: Bearer secret-token"',
  )
  expect(snippets('gemini', connection, 'macos')[0]!.code).toBe(
    'gemini mcp add --scope user --transport http -H "Authorization: Bearer secret-token" taurilanzi http://127.0.0.1:47833/mcp',
  )
})

test('Claude Desktop starts the bridge, with a Windows path escaped', () => {
  const [block] = snippets('claude-desktop', connection, 'windows')
  expect(block!.code).toContain('"C:\\\\Program Files\\\\App\\\\app.exe"')
  expect(JSON.parse(block!.code)).toEqual({
    mcpServers: {
      taurilanzi: {
        command: connection.bridge,
        args: ['--mcp-bridge', connection.url],
        env: { TAURILANZI_MCP_TOKEN: 'secret-token' },
      },
    },
  })
})

test('Cursor and VS Code read the address and the header from JSON', () => {
  expect(JSON.parse(snippets('cursor', connection, 'macos')[0]!.code)).toEqual({
    mcpServers: { taurilanzi: { url: connection.url, headers: { Authorization: 'Bearer secret-token' } } },
  })
  expect(JSON.parse(snippets('vscode', connection, 'macos')[0]!.code)).toEqual({
    servers: { taurilanzi: { type: 'http', url: connection.url, headers: { Authorization: 'Bearer secret-token' } } },
  })
})

test('Codex reads the token from a variable, set as each system does', () => {
  const [config, variable] = snippets('codex', connection, 'linux')
  expect(config!.code).toBe('[mcp_servers.taurilanzi]\nurl = "http://127.0.0.1:47833/mcp"\nbearer_token_env_var = "TAURILANZI_MCP_TOKEN"')
  expect(variable!.code).toBe('export TAURILANZI_MCP_TOKEN="secret-token"')
  expect(tokenVariable('secret-token', 'windows')).toBe('setx TAURILANZI_MCP_TOKEN "secret-token"')
})

test('assistants are named from their User-Agent', () => {
  expect(clientName('claude-code/2.0.14 (cli)')).toBe('Claude Code')
  expect(clientName('Cursor/1.7.0')).toBe('Cursor')
  expect(clientName('ureq/3.1.0')).toBeNull()
  expect(clientName('python-httpx/0.28.1')).toBe('python-httpx')
})

test('how long ago is said in words', () => {
  const now = new Date('2026-11-19T12:00:00Z')
  expect(timeAgo(new Date('2026-11-19T11:59:40Z'), now, 'es')).toBe('ahora')
  expect(timeAgo(new Date('2026-11-19T11:55:00Z'), now, 'es')).toBe('hace 5 minutos')
  expect(timeAgo(new Date('2026-11-18T11:00:00Z'), now, 'en')).toBe('yesterday')
})

test('every assistant has its hint', () => {
  for (const assistant of ASSISTANTS) {
    expect(t(`mcp.setup.hints.${assistant}`, { variable: 'X' })).not.toBe(`mcp.setup.hints.${assistant}`)
  }
})
