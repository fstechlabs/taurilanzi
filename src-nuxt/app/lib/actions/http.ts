import type { HttpBody, HttpMethod, HttpRequest } from '~/types/engine'
// Relative, not `~`: the unit tests import this file and bun does not know that alias.
import { t } from '../i18n'

export const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']

/** Offered while typing a header name. */
export const COMMON_HEADERS = ['Authorization', 'Content-Type', 'Accept', 'X-API-Key', 'User-Agent', 'Cache-Control', 'Cookie']

export type BodyKind = HttpBody['type']

const TIMEOUT_MS = { min: 1000, max: 120000 }
const WEB_URL = /^https?:\/\/[^/\s]\S*$/
const HEADER_NAME = /^[\w!#$%&'*+.^`|~-]+$/

/** A GET to fill in, which shows on the key whether it worked. */
export function newHttpRequest(): Required<HttpRequest> {
  return { method: 'GET', url: '', headers: [], body: { type: 'none' }, timeout_ms: 10000, insecure: false, show_result: true }
}

/** An empty body of each kind, keeping the text of the one it replaces where it fits. */
export function bodyOf(kind: BodyKind, previous: HttpBody | undefined): HttpBody {
  const text = previous?.type === 'json' ? previous.json : previous?.type === 'text' ? previous.text : ''
  switch (kind) {
    case 'none':
      return { type: 'none' }
    case 'json':
      return { type: 'json', json: text || '{\n  \n}' }
    case 'text':
      return { type: 'text', text, content_type: 'text/plain' }
    case 'form':
      return { type: 'form', fields: previous?.type === 'form' ? previous.fields : [{ name: '', value: '' }] }
  }
}

export function jsonError(json: string): string | null {
  try {
    JSON.parse(json)
    return null
  }
  catch (err) {
    return (err as Error).message
  }
}

/** The JSON indented, or as it was if it does not parse. */
export function prettyJson(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2)
  }
  catch {
    return json
  }
}

/** What keeps the request from being saved, the same checks the engine makes. */
export function httpError(request: HttpRequest): string | null {
  if (!WEB_URL.test(request.url.trim())) {
    return t(request.url.trim() ? 'actions.http.errors.badAddress' : 'actions.http.errors.noAddress')
  }
  const headers = request.headers ?? []
  if (headers.some(h => !HEADER_NAME.test(h.name))) {
    return t('actions.http.errors.headerName')
  }
  if (headers.some(h => /[\r\n]/.test(h.value))) {
    return t('actions.http.errors.headerLines')
  }
  const timeout = request.timeout_ms ?? 10000
  if (timeout < TIMEOUT_MS.min || timeout > TIMEOUT_MS.max) {
    return t('actions.http.errors.timeout')
  }
  const body = request.body ?? { type: 'none' }
  if (body.type === 'json' && jsonError(body.json)) {
    return t('actions.http.errors.json')
  }
  if (body.type === 'text' && !body.content_type.trim()) {
    return t('actions.http.errors.textType')
  }
  if (body.type === 'form' && body.fields.some(f => !f.name.trim())) {
    return t('actions.http.errors.formField')
  }
  return null
}

/** One line with the method and address, another with the rest, for the list of actions. */
export function httpSummary(request: HttpRequest): { title: string, detail: string } {
  const headers = request.headers?.length ?? 0
  const body = request.body ?? { type: 'none' }
  const parts = [
    headers ? t('actions.http.summary.headers', headers) : t('actions.http.summary.noHeaders'),
    t(`actions.http.summary.body.${body.type}`),
  ]
  return { title: `${request.method} ${request.url.trim() || t('actions.http.summary.noAddress')}`, detail: parts.join(' · ') }
}
