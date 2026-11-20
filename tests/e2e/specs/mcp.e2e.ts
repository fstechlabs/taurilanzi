import { createServer, request } from 'node:http'
import { $, $$, browser, expect } from '@wdio/globals'
import { KEY_COUNT, MCP_PORT, STORE_PORT, button, engine, homePage, named, node, shot, state } from '../helpers'

const URL = `http://127.0.0.1:${MCP_PORT}/mcp`
const MASK = '•'.repeat(16)
const STORE_PLUGIN = 'com.taurilanzi.e2e.ulanziPlugin'

interface Answer {
  status: number
  body: any
}

/** What an assistant sends over Streamable HTTP. A connection of its own each time, so none
 * outlives the listener it was opened to when the server restarts. */
function post(token: string | null, method: string, params: unknown = {}): Promise<Answer> {
  const body = JSON.stringify({ jsonrpc: '2.0', id: 1, method, params })
  const headers: Record<string, string> = {
    'accept': 'application/json, text/event-stream',
    'content-type': 'application/json',
    'user-agent': 'taurilanzi-e2e/1',
  }
  if (token !== null) {
    headers.authorization = `Bearer ${token}`
  }
  return new Promise((resolve, reject) => {
    const req = request(URL, { method: 'POST', headers, agent: false }, (res) => {
      let text = ''
      res.setEncoding('utf8')
      res.on('data', (chunk) => {
        text += chunk
      })
      res.on('end', () => {
        let parsed: unknown = text
        try {
          parsed = JSON.parse(text)
        }
        catch {}
        resolve({ status: res.statusCode ?? 0, body: parsed })
      })
    })
    req.on('error', reject)
    req.end(body)
  })
}

async function rpc(token: string, method: string, params: unknown = {}) {
  const answer = await post(token, method, params)
  expect(answer.status).toBe(200)
  expect(answer.body.error).toBeUndefined()
  return answer.body.result
}

/** Whether nothing listens on the MCP port. */
async function unreachable() {
  return await post(null, 'tools/list').then(() => false, () => true)
}

/** The JSON a tool answered; it fails on a refusal, with its reason. */
async function tool(token: string, name: string, args: Record<string, unknown> = {}) {
  const result = await rpc(token, 'tools/call', { name, arguments: args })
  const text = result.content[0].text as string
  if (result.isError) {
    throw new Error(`${name} refused: ${text}`)
  }
  try {
    return JSON.parse(text)
  }
  catch {
    return text
  }
}

/** The community store with one plugin, enough for the assistant to recommend it. */
function storeServer() {
  const catalog = {
    plugins: [{
      id: STORE_PLUGIN,
      name: 'Contador',
      author: 'Taurilanzi',
      version: '1.0.0',
      description: 'Cuenta pulsaciones.',
      platforms: [],
      downloadUrl: `http://127.0.0.1:${STORE_PORT}/package.zip`,
    }],
  }
  return createServer((req, res) => {
    const found = req.url === '/catalog.json'
    res.writeHead(found ? 200 : 404, { 'content-type': found ? 'application/json' : 'text/plain' })
    res.end(found ? JSON.stringify(catalog) : '')
  })
}

const sidebar = () => $('[data-sidebar-mcp]')
const status = () => $('[data-mcp-status]')
const tokenInput = () => $('[data-mcp-token]')

async function openDialog() {
  await sidebar().click()
  await $('[data-mcp-dialog]').waitForDisplayed()
}

async function closeDialog() {
  await button('Cerrar', $('[data-mcp-dialog]')).click()
  await $('[data-mcp-dialog]').waitForExist({ reverse: true })
}

async function expandToken() {
  await $('[data-mcp-token-toggle]').click()
  await tokenInput().waitForDisplayed()
}

/** The token the dialog shows once revealed. */
async function shownToken() {
  if ((await tokenInput().getValue()) === MASK) {
    await named('Mostrar el token').click()
  }
  await browser.waitUntil(async () => (await tokenInput().getValue()) !== MASK, { timeoutMsg: 'the token was not shown' })
  return await tokenInput().getValue()
}

describe('the MCP server for AI assistants', () => {
  const store = storeServer()
  let token = ''

  before(async () => {
    await new Promise<void>(resolve => store.listen(STORE_PORT, '127.0.0.1', resolve))
  })

  after(async () => {
    await engine('set_mcp_enabled', { enabled: false })
    store.close()
  })

  it('is off until the user turns it on', async () => {
    await expect(sidebar()).toHaveAttribute('data-mcp-state', 'off')
    await expect(sidebar()).toHaveText(expect.stringContaining('IA · MCP'))
    await openDialog()
    await expect(status()).toHaveAttribute('data-mcp-state', 'off')
    await expect(status()).toHaveText('Apagado. Ningún asistente puede conectarse.')
    await expect($('[data-mcp-switch]')).toHaveAttribute('aria-checked', 'false')
    await expect($('[data-mcp-token-toggle]')).not.toBeExisting()
    await expect(tokenInput()).not.toBeExisting()
    await expect($('[data-mcp-snippet]')).not.toBeExisting()
    await shot('mcp-off')
    expect(await unreachable()).toBe(true)
  })

  it('turns on and shows how to connect each assistant, the token hidden', async () => {
    await $('[data-mcp-switch]').click()
    await expect(status()).toHaveAttribute('data-mcp-state', 'running')
    await expect(status()).toHaveText(`Activo en ${URL}`)
    await expect(sidebar()).toHaveAttribute('data-mcp-state', 'running')

    const tabs = await $$('[data-mcp-assistant]').map(tab => tab.getText())
    expect(tabs).toEqual(['Claude Code', 'Codex CLI', 'Gemini CLI', 'Claude Desktop', 'Cursor', 'VS Code', 'Otro'])
    const snippet = $('[data-mcp-snippet="command"]')
    await expect(snippet).toHaveText(expect.stringContaining(`claude mcp add --transport http --scope user taurilanzi ${URL}`))
    await expect(snippet).toHaveText(expect.stringContaining(`Authorization: Bearer ${MASK}`))

    await expect($('[data-mcp-token-toggle]')).toBeDisplayed()
    await expect(tokenInput()).not.toBeDisplayed()
    await expandToken()
    await expect(tokenInput()).toHaveValue(MASK)
    await shot('mcp-on')
    token = await shownToken()
    expect(token).toMatch(/^[0-9a-f]{64}$/)
    await expect(snippet).toHaveText(expect.stringContaining(`Authorization: Bearer ${token}`))
    await expect(named('Ocultar el token')).toBeDisplayed()

    await $('[data-mcp-assistant="cursor"]').click()
    await expect($('[data-mcp-snippet="config"]')).toHaveText(expect.stringContaining(`"url": "${URL}"`))
    await closeDialog()
  })

  it('lets an assistant with the token change the configuration live, and the user undo it', async () => {
    const init = await rpc(token, 'initialize', {
      protocolVersion: '2025-11-25',
      capabilities: {},
      clientInfo: { name: 'taurilanzi-e2e', version: '1' },
    })
    expect(init.instructions).toContain('apply_changes')
    const tools = (await rpc(token, 'tools/list')).tools.map((t: { name: string }) => t.name)
    expect(tools).toEqual(expect.arrayContaining(['get_overview', 'apply_changes', 'recommend_plugin', 'undo']))

    const snapshot = await state()
    const overview = await tool(token, 'get_overview')
    expect(overview.active_profile).toBe(snapshot.active_profile)
    const active = overview.profiles.find((p: { active: boolean }) => p.active)
    expect(active.home_page).toBe(snapshot.home_page)
    const home = homePage(snapshot)
    const free = Array.from({ length: KEY_COUNT }, (_, i) => i).find(i => !home.keys[i])!
    expect(free).toBeDefined()

    const applied = await tool(token, 'apply_changes', {
      changes: [
        { op: 'create_page', profile: overview.active_profile, page_name: 'Escenas', name: '$s' },
        { op: 'set_key', page: home.id, index: free, key: { kind: 'action', actions: [{ type: 'open_page', page: '$s' }] } },
      ],
    })
    const scenes = applied.created.$s as string
    expect(scenes).toBeTruthy()

    const link = $(`.vue-flow__edge[data-id="${home.id}:${free}"]`)
    await expect(node('Escenas')).toBeDisplayed()
    await expect(link).toBeExisting()
    const after = await state()
    expect(after.pages.map(p => p.id)).toContain(scenes)
    expect(homePage(after).keys[free]?.actions[0]).toMatchObject({ type: 'open_page', page: scenes })

    const toast = $('[data-toast]')
    await expect(toast).toHaveText(expect.stringContaining('La IA ha cambiado la configuración'))
    const undo = toast.$('[data-toast-action]')
    await expect(undo).toHaveText('Deshacer')
    await shot('mcp-edited')
    await undo.click()

    await expect(node('Escenas')).not.toBeExisting()
    await expect(link).not.toBeExisting()
    const undone = await state()
    expect(undone.pages.map(p => p.id)).not.toContain(scenes)
    expect(homePage(undone).keys[free]).toBeUndefined()
    await expect($('[data-toast]')).not.toBeExisting()
  })

  it('refuses an assistant without the right token', async () => {
    expect((await post('not-the-token', 'tools/list')).status).toBe(401)
    expect((await post(null, 'tools/list')).status).toBe(401)
  })

  it('opens the store on a plugin the assistant recommends', async () => {
    const found = await tool(token, 'search_plugin_store', { search: 'contador' })
    expect(found.map((p: { id: string }) => p.id)).toEqual([STORE_PLUGIN])
    const said = await tool(token, 'recommend_plugin', { id: STORE_PLUGIN })
    expect(said).toContain('Contador')

    await $('[data-store-dialog]').waitForDisplayed()
    await expect($('[data-store-recommended]')).toHaveText('Tu asistente de IA te recomienda Contador.')
    await $(`[data-store-plugin="${STORE_PLUGIN}"]`).waitForDisplayed({ timeout: 15_000 })
    await expect($('[data-store-details]')).toHaveText(expect.stringContaining('Contador'))
    await shot('mcp-recommended')
    await browser.keys('Escape')
    await $('[data-store-dialog]').waitForExist({ reverse: true })
  })

  it('lets in only the new token once the user makes another', async () => {
    await openDialog()
    await expandToken()
    await $('[data-mcp-regenerate]').click()
    const confirm = $('[data-mcp-regenerate-confirm]')
    await confirm.waitForDisplayed()
    await expect(confirm).toHaveText(expect.stringContaining('¿Generar un token nuevo?'))
    await shot('mcp-regenerate')
    await button('Generar otro', confirm).click()
    await confirm.waitForExist({ reverse: true })

    const old = token
    await browser.waitUntil(async () => (await shownToken()) !== old, { timeoutMsg: 'the token did not change' })
    token = await shownToken()
    expect(token).toMatch(/^[0-9a-f]{64}$/)
    await expect(status()).toHaveAttribute('data-mcp-state', 'running')
    expect((await post(old, 'tools/list')).status).toBe(401)
    expect((await rpc(token, 'tools/list')).tools.length).toBeGreaterThan(0)
  })

  it('turns off, and then nothing answers', async () => {
    await $('[data-mcp-switch]').click()
    await expect(status()).toHaveAttribute('data-mcp-state', 'off')
    await expect(sidebar()).toHaveAttribute('data-mcp-state', 'off')
    await expect(tokenInput()).not.toBeExisting()
    await expect($('[data-mcp-snippet]')).not.toBeExisting()
    expect(await unreachable()).toBe(true)
    await closeDialog()
  })
})
