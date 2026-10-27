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
