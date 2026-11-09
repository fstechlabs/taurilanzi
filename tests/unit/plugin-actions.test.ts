import { expect, test } from 'bun:test'
import { Puzzle } from '@lucide/vue'
import type { PluginActionInfo, PluginCatalog, PluginInfo } from '../../src-nuxt/app/ipc/bindings'
import { catalogOf, findAction, inspectorUrl, keyName, newPluginAction, pluginImage } from '../../src-nuxt/app/lib/actions/plugins'

function action(changes: Partial<PluginActionInfo> = {}): PluginActionInfo {
  return {
    uuid: 'com.taurilanzi.e2e.counter.count',
    name: 'Contar',
    tooltip: 'Suma uno',
    icon: 'images/icon.svg',
    inspector: 'inspector/inspector.html',
    supported: true,
    settings: {},
    visible: true,
    ...changes,
  }
}

function plugin(changes: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: 'com.taurilanzi.e2e.counter',
    name: 'Contador',
    author: 'Taurilanzi',
    description: '',
    version: '1.0.0',
    category: 'Pruebas',
    icon: 'images/plugin icon.svg',
    folder: 'com.taurilanzi.e2e.ulanziPlugin',
    protocol: 'ulanzi',
    runtime: 'node',
    run: { state: 'running' },
    connected: true,
    website: null,
    actions: [action()],
    ...changes,
  }
}

function catalog(plugins: PluginInfo[] = [plugin()]): PluginCatalog {
  return { port: 3906, node: 'v20.0.0', python: null, language: 'es', plugins }
}

test('keys are named by column and row like the engine', () => {
  expect(keyName(0)).toBe('0_0')
  expect(keyName(4)).toBe('4_0')
  expect(keyName(7)).toBe('2_1')
  expect(keyName(13)).toBe('3_2')
})

test('the inspector opens from the host with the query of Ulanzi Studio', () => {
  const url = new URL(inspectorUrl(catalog(), plugin(), action(), 7, 'abc')!)
  expect(url.origin).toBe('http://127.0.0.1:3906')
  expect(url.pathname).toBe('/plugins/com.taurilanzi.e2e.ulanziPlugin/inspector/inspector.html')
  expect(Object.fromEntries(url.searchParams)).toEqual({
    address: '127.0.0.1',
    port: '3906',
    wsport: '3906',
    uuid: 'com.taurilanzi.e2e.counter.count',
    key: '2_1',
    actionid: 'abc',
    actionId: 'abc',
    language: 'es',
    controller: 'Keypad',
    device: 'D200',
  })
  const spaced = inspectorUrl(catalog(), plugin({ folder: 'my plugin.ulanziPlugin' }), action({ inspector: './pi/my page.html' }), 0, 'x')!
  expect(spaced).toStartWith('http://127.0.0.1:3906/plugins/my%20plugin.ulanziPlugin/pi/my%20page.html?')
  expect(inspectorUrl(catalog(), plugin(), action({ inspector: null }), 0, 'x')).toBeNull()
})

test('finds an action and its plugin in the catalog', () => {
  const list = catalog()
  expect(findAction(list, 'com.taurilanzi.e2e.counter.count')).toEqual({ plugin: list.plugins[0]!, action: list.plugins[0]!.actions[0]! })
  expect(findAction(list, 'com.taurilanzi.e2e.counter.missing')).toBeNull()
  expect(findAction(list, 'com.other.plugin.id.count')).toBeNull()
  expect(findAction(null, 'com.taurilanzi.e2e.counter.count')).toBeNull()
  const stream = catalog([plugin({ id: 'com.mutedeck.plugin', protocol: 'stream_deck', actions: [action({ uuid: 'com.mutedeck.plugin.togglemute' })] })])
  expect(findAction(stream, 'com.mutedeck.plugin.togglemute')?.plugin.id).toBe('com.mutedeck.plugin')
})

test('a new plugin action has a placement of its own and no settings', () => {
  const first = newPluginAction('com.taurilanzi.e2e.counter.count')
  const second = newPluginAction('com.taurilanzi.e2e.counter.count')
  expect(first).toMatchObject({ type: 'plugin', action: 'com.taurilanzi.e2e.counter.count', settings: {} })
  expect(first.type === 'plugin' && second.type === 'plugin' && first.id !== second.id).toBe(true)
  expect(first.type === 'plugin' && first.id).toMatch(/^[0-9a-f-]{36}$/)
})

test('each plugin with actions is a section of the catalog', () => {
  const images = plugin({
    id: 'com.taurilanzi.e2e.images',
    name: 'Imágenes',
    author: '',
    category: '',
    actions: [action({ uuid: 'com.taurilanzi.e2e.images.show', tooltip: '', icon: null })],
  })
  const empty = plugin({ id: 'com.taurilanzi.e2e.empty', actions: [] })
  const { sections, entries } = catalogOf(catalog([plugin(), images, empty]))
  expect(sections).toEqual([
    { id: 'plugin:com.taurilanzi.e2e.counter', name: 'Pruebas', description: 'Plugin de Taurilanzi' },
    { id: 'plugin:com.taurilanzi.e2e.images', name: 'Imágenes', description: 'Plugin' },
  ])
  expect(entries.map(e => [e.id, e.section, e.name, e.description])).toEqual([
    ['plugin:com.taurilanzi.e2e.counter.count', 'plugin:com.taurilanzi.e2e.counter', 'Contar', 'Suma uno'],
    ['plugin:com.taurilanzi.e2e.images.show', 'plugin:com.taurilanzi.e2e.images', 'Contar', 'Imágenes'],
  ])
  const [counter, shown] = entries
  expect(counter).toMatchObject({ icon: Puzzle, plugin: true, modal: true, soon: undefined, keywords: ['plugin', 'Contador', 'Taurilanzi'] })
  expect(counter!.create!({ otherPages: [], profiles: [] })).toMatchObject({ type: 'plugin', action: 'com.taurilanzi.e2e.counter.count' })
  expect(counter!.matches!({ type: 'plugin', action: 'com.taurilanzi.e2e.counter.count', id: 'x', settings: {} })).toBe(true)
  expect(counter!.matches!({ type: 'plugin', action: 'com.taurilanzi.e2e.images.show', id: 'x', settings: {} })).toBe(false)
  expect(counter!.matches!({ type: 'back' })).toBe(false)
  expect(shown!.image).toBe(pluginImage('com.taurilanzi.e2e.images', 'images/plugin icon.svg'))
})

test('actions a plugin hides are known on keys but never offered', () => {
  const tile = action({ uuid: 'com.taurilanzi.e2e.game.tile', visible: false })
  const game = plugin({ id: 'com.taurilanzi.e2e.game', actions: [tile] })
  const { sections, entries } = catalogOf(catalog([plugin(), game]))
  expect(sections.map(s => s.id)).toEqual(['plugin:com.taurilanzi.e2e.counter'])
  const hidden = entries.find(e => e.id === 'plugin:com.taurilanzi.e2e.game.tile')
  expect(hidden).toMatchObject({ hidden: true, plugin: true })
  expect(entries.find(e => e.id === 'plugin:com.taurilanzi.e2e.counter.count')!.hidden).toBe(false)
  expect(findAction(catalog([game]), 'com.taurilanzi.e2e.game.tile')?.action).toBe(tile)
})

test('actions that cannot run here are shown but not offered', () => {
  const dial = action({ uuid: 'com.taurilanzi.e2e.counter.dial', supported: false })
  const reason = 'Hace falta Node 20 o posterior'
  const { entries } = catalogOf(catalog([
    plugin({ actions: [action(), dial] }),
    plugin({ id: 'com.taurilanzi.e2e.python', run: { state: 'unavailable', reason }, actions: [action({ uuid: 'com.taurilanzi.e2e.python.run' })] }),
    plugin({ id: 'com.taurilanzi.e2e.stopped', run: { state: 'restarting', restarts: 2 }, actions: [action({ uuid: 'com.taurilanzi.e2e.stopped.run' })] }),
  ]))
  const [runs, turns, python, restarting] = entries.map(e => e.soon)
  expect(runs).toBeUndefined()
  expect(turns).toContain('diales')
  expect(python).toBe(reason)
  expect(restarting).toBeUndefined()
})

test('images of plugins encode their paths', () => {
  expect(pluginImage('com.taurilanzi.e2e.counter', 'images/icon.svg')).toBe('deck://localhost/plugin/com.taurilanzi.e2e.counter/images/icon.svg')
  expect(pluginImage('com.taurilanzi.e2e.counter', './images/my icon#1.png')).toBe('deck://localhost/plugin/com.taurilanzi.e2e.counter/images/my%20icon%231.png')
  expect(pluginImage('a b', '/x.png')).toBe('deck://localhost/plugin/a%20b/x.png')
  expect(pluginImage('com.taurilanzi.e2e.counter', null)).toBeUndefined()
  expect(pluginImage('com.taurilanzi.e2e.counter', '')).toBeUndefined()
})

test('Stream Deck inspectors get the settings of their key, and new keys those of the manifest', () => {
  const stream = plugin({ protocol: 'stream_deck' })
  const url = new URL(inspectorUrl(catalog([stream]), stream, action(), 0, 'x', { step: 2 })!)
  expect(JSON.parse(url.searchParams.get('settings')!)).toEqual({ step: 2 })
  expect(new URL(inspectorUrl(catalog(), plugin(), action(), 0, 'x', { step: 2 })!).searchParams.has('settings')).toBe(false)
  const settings = { step: 1 }
  const created = newPluginAction('com.a.b.act', settings)
  expect(created).toMatchObject({ type: 'plugin', action: 'com.a.b.act', settings: { step: 1 } })
  expect(created.type === 'plugin' && created.settings).not.toBe(settings)
})
