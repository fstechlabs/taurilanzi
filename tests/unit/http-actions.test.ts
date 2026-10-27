import { expect, test } from 'bun:test'
import { bodyOf, httpError, httpSummary, jsonError, newHttpRequest, prettyJson } from '../../src-nuxt/app/lib/actions/http'
import type { HttpRequest } from '../../src-nuxt/app/ipc/bindings'

function request(changes: Partial<HttpRequest> = {}): HttpRequest {
  return { ...newHttpRequest(), url: 'https://example.com/api', ...changes }
}

test('a new request is a GET that shows its result and needs an address', () => {
  expect(newHttpRequest()).toEqual({ method: 'GET', url: '', headers: [], body: { type: 'none' }, timeout_ms: 10000, insecure: false, show_result: true })
  expect(httpError(newHttpRequest())).toBe('Escribe la dirección')
  expect(httpError(request())).toBeNull()
})

test('checks the address, headers, time limit and body like the engine', () => {
  expect(httpError(request({ url: 'ftp://example.com' }))).toContain('http://')
  expect(httpError(request({ url: 'https:///x' }))).toContain('http://')
  expect(httpError(request({ headers: [{ name: 'X-API-Key', value: 'a' }] }))).toBeNull()
  expect(httpError(request({ headers: [{ name: 'Bad Name', value: 'a' }] }))).toContain('cabecera')
  expect(httpError(request({ headers: [{ name: '', value: 'a' }] }))).toContain('cabecera')
  expect(httpError(request({ headers: [{ name: 'X', value: 'a\nb' }] }))).toContain('varias líneas')
  expect(httpError(request({ timeout_ms: 500 }))).toContain('120 segundos')
  expect(httpError(request({ body: { type: 'json', json: '{' } }))).toContain('JSON')
  expect(httpError(request({ body: { type: 'text', text: 'x', content_type: ' ' } }))).toContain('tipo de texto')
  expect(httpError(request({ body: { type: 'form', fields: [{ name: '', value: 'x' }] } }))).toContain('nombre')
})

test('switching the body keeps the text where it fits', () => {
  expect(bodyOf('text', { type: 'json', json: '{"a":1}' })).toEqual({ type: 'text', text: '{"a":1}', content_type: 'text/plain' })
  expect(bodyOf('json', { type: 'text', text: '[1]', content_type: 'text/plain' })).toEqual({ type: 'json', json: '[1]' })
  expect(bodyOf('json', { type: 'none' }).type).toBe('json')
  expect(bodyOf('form', { type: 'none' })).toEqual({ type: 'form', fields: [{ name: '', value: '' }] })
  expect(bodyOf('none', { type: 'json', json: '{}' })).toEqual({ type: 'none' })
})

test('formats and checks JSON', () => {
  expect(jsonError('{"a":1}')).toBeNull()
  expect(jsonError('{')).not.toBeNull()
  expect(prettyJson('{"a":[1,2]}')).toBe('{\n  "a": [\n    1,\n    2\n  ]\n}')
  expect(prettyJson('not json')).toBe('not json')
})

test('sums a request up in two lines for the list of actions', () => {
  expect(httpSummary(request({ method: 'POST', headers: [{ name: 'A', value: 'b' }], body: { type: 'json', json: '{}' } })))
    .toEqual({ title: 'POST https://example.com/api', detail: '1 cabecera · cuerpo JSON' })
  expect(httpSummary(newHttpRequest())).toEqual({ title: 'GET (sin dirección)', detail: 'sin cabeceras · sin cuerpo' })
  expect(httpSummary(request({ headers: [{ name: 'A', value: 'b' }, { name: 'C', value: 'd' }], body: { type: 'form', fields: [] } })).detail)
    .toBe('2 cabeceras · formulario')
})
