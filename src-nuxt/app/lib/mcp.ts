import type { Os } from './os'

export type Assistant = 'claude-code' | 'claude-desktop' | 'cursor' | 'vscode' | 'codex' | 'gemini' | 'other'

export const ASSISTANTS: Assistant[] = ['claude-code', 'codex', 'gemini', 'claude-desktop', 'cursor', 'vscode', 'other']

/** The name each assistant goes by, the same in every language. */
export const ASSISTANT_NAMES: Record<Exclude<Assistant, 'other'>, string> = {
  'claude-code': 'Claude Code',
  'claude-desktop': 'Claude Desktop',
  'cursor': 'Cursor',
  'vscode': 'VS Code',
  'codex': 'Codex CLI',
  'gemini': 'Gemini CLI',
}

/** The variable the bridge and Codex read the token from. */
export const TOKEN_ENV = 'TAURILANZI_MCP_TOKEN'
const SERVER = 'taurilanzi'

export interface Connection {
  url: string
  token: string
  /** The app's own program, which assistants that only start commands run as a bridge. */
  bridge: string
}

export type SnippetKind = 'command' | 'config' | 'variable' | 'url' | 'header' | 'bridge'

export interface Snippet {
  kind: SnippetKind
  code: string
}

function json(value: unknown) {
  return JSON.stringify(value, null, 2)
}

/** A command line for the user's terminal: export on macOS and Linux, setx on Windows, which
 * only new terminals see. */
export function tokenVariable(token: string, os: Os) {
  return os === 'windows'
    ? `setx ${TOKEN_ENV} "${token}"`
    : `export ${TOKEN_ENV}="${token}"`
}

/** What to paste in each assistant to connect it, with the live address and token. */
export function snippets(assistant: Assistant, { url, token, bridge }: Connection, os: Os): Snippet[] {
  const bearer = `Bearer ${token}`
  switch (assistant) {
    case 'claude-code':
      return [{ kind: 'command', code: `claude mcp add --transport http --scope user ${SERVER} ${url} --header "Authorization: ${bearer}"` }]
    case 'claude-desktop':
      return [{
        kind: 'config',
        code: json({ mcpServers: { [SERVER]: { command: bridge, args: ['--mcp-bridge', url], env: { [TOKEN_ENV]: token } } } }),
      }]
    case 'cursor':
      return [{ kind: 'config', code: json({ mcpServers: { [SERVER]: { url, headers: { Authorization: bearer } } } }) }]
    case 'vscode':
      return [{ kind: 'config', code: json({ servers: { [SERVER]: { type: 'http', url, headers: { Authorization: bearer } } } }) }]
    case 'codex':
      return [
        { kind: 'config', code: `[mcp_servers.${SERVER}]\nurl = ${JSON.stringify(url)}\nbearer_token_env_var = "${TOKEN_ENV}"` },
        { kind: 'variable', code: tokenVariable(token, os) },
      ]
    case 'gemini':
      return [{ kind: 'command', code: `gemini mcp add --scope user --transport http -H "Authorization: ${bearer}" ${SERVER} ${url}` }]
    case 'other':
      return [
        { kind: 'url', code: url },
        { kind: 'header', code: `Authorization: ${bearer}` },
        { kind: 'bridge', code: `"${bridge}" --mcp-bridge ${url}` },
      ]
  }
}

const CLIENTS: [RegExp, string][] = [
  [/claude[-_ ]?code/i, 'Claude Code'],
  [/claude[-_ ]?ai/i, 'Claude Desktop'],
  [/claude/i, 'Claude'],
  [/cursor/i, 'Cursor'],
  [/vscode|visual studio code|code-oss/i, 'VS Code'],
  [/codex/i, 'Codex'],
  [/gemini/i, 'Gemini CLI'],
]

/** A readable name for the assistant behind a `User-Agent`; null for the app's own bridge
 * before the assistant behind it introduced itself. */
export function clientName(userAgent: string): string | null {
  if (/^ureq\//i.test(userAgent)) {
    return null
  }
  const known = CLIENTS.find(([pattern]) => pattern.test(userAgent))
  if (known) {
    return known[1]
  }
  return userAgent.trim().split(/[\s/]/)[0] || userAgent
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

/** How long ago something happened, in words: "hace 5 minutos", "yesterday". */
export function timeAgo(when: Date, now: Date, locale: string) {
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  const seconds = Math.max(0, (now.getTime() - when.getTime()) / 1000)
  for (const [unit, size] of UNITS) {
    if (seconds >= size) {
      return format.format(-Math.floor(seconds / size), unit)
    }
  }
  return format.format(0, 'second')
}
