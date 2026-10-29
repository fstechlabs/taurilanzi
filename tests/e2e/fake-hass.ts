import { createHash } from 'node:crypto'
import { createServer, type Server } from 'node:http'
import type { Duplex } from 'node:stream'

// Home Assistant's WebSocket API as the end-to-end tests need it: signing in with a token, the
// states of a few entities, their registries, and every action asked for, kept to look at.

const WEBSOCKET_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'

export interface FakeEntity {
  state: string
  attributes: Record<string, unknown>
  device?: string
}

export interface Called {
  domain: string
  service: string
  data: Record<string, unknown>
  target: Record<string, unknown>
}

export class FakeHass {
  readonly calls: Called[] = []
  private server: Server | null = null
  private readonly sockets = new Set<Duplex>()

  constructor(
    readonly port: number,
    readonly token: string,
    readonly entities: Record<string, FakeEntity>,
  ) {}

  get url() {
    return `http://127.0.0.1:${this.port}`
  }

  async start() {
    this.server = createServer((_req, res) => {
      res.writeHead(404)
      res.end()
    })
    this.server.on('upgrade', (req, socket) => {
      const key = req.headers['sec-websocket-key']
      if (req.url !== '/api/websocket' || typeof key !== 'string') {
        socket.destroy()
        return
      }
      const accept = createHash('sha1').update(key + WEBSOCKET_GUID).digest('base64')
      socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`)
      this.serve(socket)
    })
    await new Promise<void>(resolve => this.server!.listen(this.port, '127.0.0.1', resolve))
  }

  async stop() {
    for (const socket of this.sockets) {
      socket.destroy()
    }
    await new Promise<void>(resolve => this.server?.close(() => resolve()) ?? resolve())
  }

  private serve(socket: Duplex) {
    this.sockets.add(socket)
    socket.on('close', () => this.sockets.delete(socket))
    socket.on('error', () => socket.destroy())
    const send = (message: unknown) => socket.write(frame(Buffer.from(JSON.stringify(message))))
    let signedIn = false
    let pending = Buffer.alloc(0)
    socket.on('data', (chunk: Buffer) => {
      pending = Buffer.concat([pending, chunk])
      for (;;) {
        const read = unframe(pending)
        if (!read) {
          return
        }
        pending = pending.subarray(read.length)
        if (read.opcode === 0x8) {
          socket.end(frame(Buffer.alloc(0), 0x8))
          return
        }
        if (read.opcode === 0x9) {
          socket.write(frame(read.payload, 0xA))
          continue
        }
        if (read.opcode !== 0x1) {
          continue
        }
        const message = JSON.parse(read.payload.toString('utf8'))
        if (!signedIn) {
          signedIn = message.type === 'auth' && message.access_token === this.token
          send(signedIn ? { type: 'auth_ok', ha_version: '2026.10.0' } : { type: 'auth_invalid', message: 'Invalid access token' })
          continue
        }
        this.answer(message, send)
      }
    })
    send({ type: 'auth_required', ha_version: '2026.10.0' })
  }

  private answer(message: { id: number, type: string, [field: string]: any }, send: (message: unknown) => void) {
    const result = (value: unknown) => send({ id: message.id, type: 'result', success: true, result: value })
    const ids = Object.keys(this.entities)
    switch (message.type) {
      case 'ping':
        send({ id: message.id, type: 'pong' })
        return
      case 'get_config':
        result({ unit_system: { temperature: '°C' }, location_name: 'Casa' })
        return
      case 'get_states':
        result(ids.map(id => ({ entity_id: id, state: this.entities[id]!.state, attributes: this.entities[id]!.attributes })))
        return
      case 'config/entity_registry/list_for_display':
        result({ entities: ids.map(id => ({ ei: id, di: this.entities[id]!.device ?? null })) })
        return
      case 'config/device_registry/list': {
        const devices = new Set(ids.map(id => this.entities[id]!.device).filter(Boolean))
        result([...devices].map(id => ({ id, name: id, area_id: null, disabled_by: null })))
        return
      }
      case 'frontend/get_translations':
      case 'frontend/get_icons':
        result({ resources: {} })
        return
      case 'config/area_registry/list':
      case 'config/floor_registry/list':
      case 'config/label_registry/list':
        result([])
        return
      case 'get_services':
        result({})
        return
      case 'subscribe_events':
      case 'unsubscribe_events':
        result(null)
        return
      case 'subscribe_entities': {
        result(null)
        const wanted: string[] = message.entity_ids ?? ids
        const added = Object.fromEntries(wanted.filter(id => this.entities[id]).map((id) => {
          const entity = this.entities[id]!
          return [id, { s: entity.state, a: entity.attributes, lc: Date.now() / 1000 }]
        }))
        send({ id: message.id, type: 'event', event: { a: added } })
        return
      }
      case 'call_service':
        this.calls.push({ domain: message.domain, service: message.service, data: message.service_data ?? {}, target: message.target ?? {} })
        result({ context: {} })
        return
      default:
        send({ id: message.id, type: 'result', success: false, error: { code: 'unknown_command', message: `Unknown command ${message.type}.` } })
    }
  }
}

/** A frame from the server, which goes unmasked. */
function frame(payload: Buffer, opcode = 0x1): Buffer {
  const length = payload.length
  const header = length < 126
    ? Buffer.from([0x80 | opcode, length])
    : length < 65536
      ? Buffer.from([0x80 | opcode, 126, length >> 8, length & 0xFF])
      : Buffer.concat([Buffer.from([0x80 | opcode, 127]), bigLength(length)])
  return Buffer.concat([header, payload])
}

function bigLength(length: number) {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(BigInt(length))
  return bytes
}

/** The first whole frame of a client, which always masks it; null until it has all arrived. */
function unframe(data: Buffer): { opcode: number, payload: Buffer, length: number } | null {
  if (data.length < 2) {
    return null
  }
  const opcode = data[0]! & 0x0F
  const masked = (data[1]! & 0x80) !== 0
  let length = data[1]! & 0x7F
  let offset = 2
  if (length === 126) {
    if (data.length < 4) {
      return null
    }
    length = data.readUInt16BE(2)
    offset = 4
  }
  else if (length === 127) {
    if (data.length < 10) {
      return null
    }
    length = Number(data.readBigUInt64BE(2))
    offset = 10
  }
  const mask = masked ? data.subarray(offset, offset + 4) : null
  offset += masked ? 4 : 0
  if (data.length < offset + length) {
    return null
  }
  const payload = Buffer.from(data.subarray(offset, offset + length))
  if (mask) {
    for (let i = 0; i < payload.length; i++) {
      payload[i]! ^= mask[i % 4]!
    }
  }
  return { opcode, payload, length: offset + length }
}
