// Loaded first in every page of a plugin. Ulanzi Studio lets local pages call any server; WebKit
// does not, so calls to other origins go through the host, which makes them for the page.
(() => {
  const config = window.__TAURILANZI__
  if (!config) {
    return
  }
  const here = location.origin

  const remote = (url) => {
    try {
      const parsed = new URL(url, location.href)
      return (parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.origin !== here ? parsed.href : null
    }
    catch {
      return null
    }
  }

  // Errors of a page only reach its console, which nobody sees: they go to the plugin's log.
  let reported = 0
  const report = (level, parts) => {
    if (!config.plugin || reported >= 100) {
      return
    }
    reported += 1
    const message = parts.map(part => part instanceof Error ? part.stack || part.message : typeof part === 'string' ? part : (() => {
      try {
        return JSON.stringify(part)
      }
      catch {
        return String(part)
      }
    })()).join(' ').slice(0, 1500)
    const query = new URLSearchParams({ plugin: config.plugin, level, message: `${location.pathname}: ${message}` })
    nativeFetch(`${config.log}?${query}`, { headers: { 'X-Taurilanzi-Token': config.token } }).catch(() => {})
  }
  window.addEventListener('error', event => report('error', [event.error || event.message]))
  window.addEventListener('unhandledrejection', event => report('error', ['unhandled rejection:', event.reason]))
  const consoleError = console.error.bind(console)
  console.error = (...parts) => {
    report('error', parts)
    consoleError(...parts)
  }

  const nativeFetch = window.fetch.bind(window)
  window.fetch = async (input, init = {}) => {
    const request = input instanceof Request ? input : null
    const target = remote(request ? request.url : String(input))
    if (!target) {
      return nativeFetch(input, init)
    }
    const method = (init.method || (request && request.method) || 'GET').toUpperCase()
    const headers = new Headers(init.headers || (request ? request.headers : undefined))
    headers.set('X-Taurilanzi-Token', config.token)
    headers.set('X-Taurilanzi-Target', target)
    let body = init.body
    if (body === undefined && request && method !== 'GET' && method !== 'HEAD') {
      body = await request.clone().arrayBuffer()
    }
    return nativeFetch(config.proxy, { method, headers, body, signal: init.signal || (request && request.signal) })
  }

  const open = XMLHttpRequest.prototype.open
  const send = XMLHttpRequest.prototype.send
  const setHeader = XMLHttpRequest.prototype.setRequestHeader
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    const target = remote(String(url))
    this.__taurilanziTarget = target
    return open.call(this, method, target ? config.proxy : url, ...rest)
  }
  XMLHttpRequest.prototype.send = function (body) {
    if (this.__taurilanziTarget) {
      setHeader.call(this, 'X-Taurilanzi-Token', config.token)
      setHeader.call(this, 'X-Taurilanzi-Target', this.__taurilanziTarget)
    }
    return send.call(this, body)
  }
})()

// Pages of Stream Deck and Stream Dock plugins are handed their connection by the host, which
// calls a function the page defines once it has loaded: the inspector of a key with five
// arguments, the main service with four.
;(() => {
  const config = window.__TAURILANZI__
  const deck = config && config.streamDeck
  if (!deck) {
    return
  }
  const query = new URLSearchParams(location.search)
  const context = query.get('actionid')
  // Looked up as the page's own scripts see them: some declare these with `const`.
  const global = name => {
    try {
      return (0, eval)(`typeof ${name} === 'function' ? ${name} : null`)
    }
    catch {
      return null
    }
  }

  if (context) {
    const action = query.get('uuid') || ''
    const key = query.get('key') || '0_0'
    const [column, row] = key.split('_').map(Number)
    let settings = {}
    try {
      settings = JSON.parse(query.get('settings') || '{}')
    }
    catch {}
    // The host learns which key an inspector is for from its registration.
    const send = WebSocket.prototype.send
    WebSocket.prototype.send = function (data) {
      if (typeof data === 'string' && data.includes('registerPropertyInspector')) {
        try {
          const message = JSON.parse(data)
          if (message.event === 'registerPropertyInspector') {
            message.taurilanzi = { action, key, settings }
            data = JSON.stringify(message)
          }
        }
        catch {}
      }
      return send.call(this, data)
    }
    // Stream Dock's inspectors ask the host for files and wait for their paths, which a page
    // cannot read from its own inputs.
    document.addEventListener('click', (event) => {
      const input = event.target instanceof Element && event.target.closest('input[type="file"]')
      const answer = global('onFilePickerReturn')
      if (!input || !answer) {
        return
      }
      event.preventDefault()
      fetch(`${config.pick}?accept=${encodeURIComponent(input.accept || '')}`, { headers: { 'X-Taurilanzi-Token': config.token } })
        .then(response => response.json())
        .then((paths) => {
          if (paths.length) {
            answer(JSON.stringify(paths))
          }
        })
        .catch(() => {})
    }, true)
    const actionInfo = {
      action,
      context,
      device: deck.device,
      payload: { settings, coordinates: { column, row }, controller: 'Keypad', state: deck.state || 0, isInMultiAction: false, resources: {} },
    }
    connect(context, 'registerPropertyInspector', JSON.stringify(actionInfo))
  }
  else {
    connect(deck.plugin, 'registerPlugin')
  }

  function connect(uuid, register, actionInfo) {
    window.addEventListener('load', () => {
      // The one Stream Dock calls first, then the others pages define.
      const own = deck.mirabox ? 'connectMiraBoxSDSocket' : 'connectElgatoStreamDeckSocket'
      const names = [own, 'connectElgatoStreamDeckSocket', 'connectMiraBoxSDSocket', 'connectSocket', 'connectSDSocket']
      const found = names.map(global).find(Boolean)
      if (!found) {
        return
      }
      const args = [String(deck.port), uuid, register, JSON.stringify(deck.info)]
      if (actionInfo) {
        args.push(actionInfo)
      }
      found(...args)
    })
  }
})()
