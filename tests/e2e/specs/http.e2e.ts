import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { $, browser, expect } from '@wdio/globals'
import {
  KEY_COUNT,
  addAction,
  button,
  chooseOption,
  clickOn,
  fill,
  homePage,
  node,
  saved,
  shot,
  state,
} from '../helpers'

interface Received {
  method: string
  url: string
  headers: Record<string, string | string[] | undefined>
  body: string
}

/** A server on this machine that answers with what it got, or fails under /fail. */
function echoServer() {
  const received: Received[] = []
  const server = createServer((req, res) => {
    let body = ''
    req.on('data', (chunk) => {
      body += chunk
    })
    req.on('end', () => {
      received.push({ method: req.method!, url: req.url!, headers: req.headers, body })
      res.writeHead(req.url!.startsWith('/fail') ? 500 : 200, { 'content-type': 'application/json' })
      res.end(JSON.stringify({ ok: true, method: req.method, body }))
    })
  })
  return { server, received }
}

async function storedKey(index: number) {
  return homePage(await state()).keys[index]
}

async function keyImage(index: number) {
  const snapshot = await state()
  return await browser.execute(async (page, index) => {
    const bytes = new Uint8Array(await (await fetch(`deck://localhost/key/${page}/${index}?t=${Date.now()}`)).arrayBuffer())
    return bytes.join(',')
  }, snapshot.home_page, index)
}

describe('the HTTP request action', () => {
  const { server, received } = echoServer()
  let base: string
  let index: number

  before(async () => {
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`
    const home = homePage(await state())
    index = [...Array(KEY_COUNT).keys()].reverse().find(i => !home.keys[i])!
    await clickOn(node(home.name).$(`button[aria-label="Tecla ${index + 1}"]`))
    await button('Configurar tecla').click()
  })

  after(() => {
    server.close()
  })

  it('builds a POST with headers and a JSON body in its own dialog', async () => {
    await addAction('http')
    await expect($('[data-action-dialog]')).toBeDisplayed()
    await expect($('[data-save-status]')).toHaveText(expect.stringContaining('escribe la dirección'))

    await chooseOption('Método', 'POST')
    await fill($('[aria-label="Dirección de la petición"]'), `${base}/api/luz?x=1`)
    await button('Añadir cabecera').click()
    await fill($('[aria-label="Nombre de la cabecera 1"]'), 'Authorization')
    await fill($('[aria-label="Valor de la cabecera 1"]'), 'Bearer abc')
    await clickOn($('//*[@aria-label="Tipo de cuerpo"]//button[normalize-space()="JSON"]'))
    await $('[aria-label="Cuerpo JSON"]').setValue('{"on": tru')
    await expect($('[data-save-status]')).toHaveText(expect.stringContaining('JSON válido'))
    await $('[aria-label="Cuerpo JSON"]').setValue('{"on": true}')
    await saved()

    expect((await storedKey(index))!.actions).toEqual([{
      type: 'http',
      request: {
        method: 'POST',
        url: `${base}/api/luz?x=1`,
        headers: [{ name: 'Authorization', value: 'Bearer abc' }],
        body: { type: 'json', json: '{"on": true}' },
        timeout_ms: 10000,
        insecure: false,
        show_result: true,
      },
    }])
  })

  it('tries the request from the editor and shows the answer', async () => {
    await button('Probar petición').click()
    const answer = $('[data-http-answer]')
    await expect(answer).toBeDisplayed()
    await expect(answer).toHaveText(expect.stringContaining('200 OK'))
    const last = received.at(-1)!
    expect(last).toMatchObject({ method: 'POST', url: '/api/luz?x=1', body: '{"on": true}' })
    expect(last.headers.authorization).toBe('Bearer abc')
    expect(last.headers['content-type']).toBe('application/json')
    await shot('http-answer')
    await button('Hecho').click()
    await $('[data-action-dialog]').waitForExist({ reverse: true })
    await expect($('[data-action-summary]')).toHaveText(expect.stringContaining(`POST ${base}/api/luz?x=1`))
    await expect($('[data-action-summary]')).toHaveText(expect.stringContaining('1 cabecera · cuerpo JSON'))
    await shot('http-summary')
  })

  it('sends it when the key is pressed and shows how it went on the key', async () => {
    const before = await keyImage(index)
    const count = received.length
    await button('Probar tecla').waitForEnabled()
    await button('Probar tecla').click()
    await browser.waitUntil(async () => received.length > count, { timeoutMsg: 'the key did not send the request' })
    await browser.waitUntil(async () => (await keyImage(index)) !== before, { timeout: 1000, timeoutMsg: 'the key did not show the result' })
    await browser.waitUntil(async () => (await keyImage(index)) === before, { timeout: 4000, timeoutMsg: 'the result did not go away' })
  })

  it('shows a failing answer and sends a form', async () => {
    await button('Configurar Petición HTTP').click()
    await expect($('[data-action-dialog]')).toBeDisplayed()
    await fill($('[aria-label="Dirección de la petición"]'), `${base}/fail`)
    await clickOn($('//*[@aria-label="Tipo de cuerpo"]//button[normalize-space()="Formulario"]'))
    await fill($('[aria-label="Nombre del campo 1"]'), 'nombre del campo')
    await fill($('[aria-label="Valor del campo 1"]'), 'ñ & más')
    await saved()
    await button('Probar petición').click()
    await expect($('[data-http-answer]')).toHaveText(expect.stringContaining('500 Internal Server Error'))
    const last = received.at(-1)!
    expect(last.body).toBe('nombre+del+campo=%C3%B1+%26+m%C3%A1s')
    expect(last.headers['content-type']).toBe('application/x-www-form-urlencoded')
    await shot('http-failed')
  })

  it('says why a request could not be made', async () => {
    await fill($('[aria-label="Dirección de la petición"]'), 'http://127.0.0.1:9/nadie')
    await saved()
    await button('Probar petición').click()
    await expect($('[data-http-error]')).toHaveText(expect.stringContaining('No se pudo hacer la petición'))
  })
})
