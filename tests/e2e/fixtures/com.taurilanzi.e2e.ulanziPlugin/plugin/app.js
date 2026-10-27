// The main service of the test plugin. It speaks Ulanzi's protocol with a WebSocket client of its
// own, so it needs nothing installed and runs on the Node 20 that Ulanzi Studio ships.
import { randomBytes } from 'node:crypto'
import { connect as tcp } from 'node:net'

const PLUGIN = 'com.taurilanzi.e2e.counter'
const [address = '127.0.0.1', port = '3906'] = process.argv.slice(2)
const settings = new Map()

function socket(onOpen, onText) {
  const conn = tcp(Number(port), address)
  let buffer = Buffer.alloc(0)
  let open = false
  conn.write([
    'GET / HTTP/1.1',
    `Host: ${address}:${port}`,
    'Upgrade: websocket',
    'Connection: Upgrade',
    `Sec-WebSocket-Key: ${randomBytes(16).toString('base64')}`,
    'Sec-WebSocket-Version: 13',
    '',
    '',
  ].join('\r\n'))
  conn.on('data', (chunk) => {
    buffer = Buffer.concat([buffer, chunk])
    if (!open) {
      const end = buffer.indexOf('\r\n\r\n')
      if (end < 0) {
        return
      }
      buffer = buffer.subarray(end + 4)
      open = true
      onOpen()
    }
    while (buffer.length >= 2) {
      let length = buffer[1] & 0x7f
      let at = 2
      if (length === 126) {
        length = buffer.readUInt16BE(2)
        at = 4
      }
      else if (length === 127) {
        length = Number(buffer.readBigUInt64BE(2))
        at = 10
      }
      if (buffer.length < at + length) {
        return
      }
      const opcode = buffer[0] & 0x0f
      const payload = buffer.subarray(at, at + length)
      buffer = buffer.subarray(at + length)
      if (opcode === 1) {
        onText(payload.toString('utf8'))
      }
      else if (opcode === 8) {
        process.exit(0)
      }
    }
  })
  conn.on('close', () => process.exit(0))
  conn.on('error', () => process.exit(1))
  return (text) => {
    const payload = Buffer.from(text)
    const head = payload.length < 126
      ? Buffer.from([0x81, 0x80 | payload.length])
      : Buffer.concat([Buffer.from([0x81, 0x80 | 126]), Buffer.from([payload.length >> 8, payload.length & 0xff])])
    const mask = randomBytes(4)
    const masked = Buffer.from(payload.map((b, i) => b ^ mask[i % 4]))
    conn.write(Buffer.concat([head, mask, masked]))
  }
}

const send = socket(
  () => {
    send(JSON.stringify({ code: 0, cmd: 'connected', uuid: PLUGIN }))
    send(JSON.stringify({ cmd: 'logMessage', uuid: PLUGIN, key: '', actionid: '', message: 'connected', level: 'info' }))
  },
  (text) => {
    const message = JSON.parse(text)
    if (message.code !== undefined && message.cmdType !== 'REQUEST') {
      return
    }
    send(JSON.stringify({ code: 0, ...message }))
    handle(message)
  },
)

function command(cmd, fields) {
  send(JSON.stringify({ cmd, uuid: PLUGIN, key: '', actionid: '', ...fields }))
}

function current(message) {
  return settings.get(message.actionid) ?? { count: 0 }
}

function draw(message) {
  const count = current(message).count ?? 0
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 144 144"><rect width="144" height="144" fill="#0369a1"/><text x="72" y="98" font-size="80" font-weight="700" text-anchor="middle" fill="#fff" font-family="sans-serif">${count}</text></svg>`
  command('state', {
    param: {
      statelist: [{
        uuid: message.uuid,
        key: message.key,
        actionid: message.actionid,
        type: 1,
        data: `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`,
        textData: `pulsado ${count}`,
        showtext: true,
      }],
    },
  })
}

function save(message, changes) {
  const next = { ...current(message), ...changes }
  settings.set(message.actionid, next)
  command('setSettings', { uuid: message.uuid, key: message.key, actionid: message.actionid, settings: next })
}

function handle(message) {
  switch (message.cmd) {
    case 'add':
    case 'paramfromapp':
      settings.set(message.actionid, { count: 0, ...message.param })
      draw(message)
      break
    case 'didReceiveSettings':
      settings.set(message.actionid, { count: 0, ...message.settings })
      break
    case 'run': {
      const count = (current(message).count ?? 0) + 1
      save(message, { count })
      draw(message)
      if (count === 3) {
        command('toast', { msg: 'Tres pulsaciones' })
      }
      break
    }
    case 'sendToPlugin':
      if (message.payload?.echo !== undefined) {
        command('sendToPropertyInspector', { uuid: message.uuid, key: message.key, actionid: message.actionid, payload: { pong: message.payload.echo } })
      }
      if (message.payload?.alert) {
        command('showAlert', { uuid: message.uuid, key: message.key, actionid: message.actionid })
      }
      break
    case 'clear':
      for (const gone of message.param ?? []) {
        settings.delete(gone.actionid)
      }
      break
  }
}
