import { createHash, randomUUID } from 'node:crypto'
import { createServer, type IncomingMessage, type Server } from 'node:http'

// The cloud's API for the desktop app, kept in memory, as the end-to-end tests need it: signing a
// computer in through the browser's redirects, and documents with every version and their head.

export const SETTINGS_ID = '00000000-0000-0000-0000-000000000000'

export interface Version {
  id: string
  parent: string | null
  editedAt: string
  deleted: boolean
  content: any
  device: string | null
}

export interface Document {
  id: string
  kind: 'profile' | 'settings'
  head: string
  versions: Version[]
}

export class FakeCloud {
  readonly documents = new Map<string, Document>()
  readonly assets = new Map<string, Buffer>()
  /** Every device token handed out, and whether it was revoked. */
  readonly tokens = new Map<string, { device: string, name: string, revoked: boolean }>()
  /** Refuses every upload as a full account does. */
  full = false
  private readonly challenges = new Map<string, string>()
  private server: Server | null = null

  constructor(readonly port: number) {}

  get url() {
    return `http://127.0.0.1:${this.port}`
  }

  async start() {
    this.server = createServer((req, res) => {
      this.handle(req).then(
        ({ status, body, headers }) => {
          res.writeHead(status, headers ?? (body === undefined ? {} : { 'content-type': 'application/json' }))
          res.end(body === undefined ? undefined : Buffer.isBuffer(body) ? body : JSON.stringify(body))
        },
        (err) => {
          res.writeHead(500)
          res.end(String(err))
        },
      )
    })
    await new Promise<void>(resolve => this.server!.listen(this.port, '127.0.0.1', resolve))
  }

  async stop() {
    await new Promise<void>(resolve => this.server?.close(() => resolve()) ?? resolve())
  }

  head(id: string): Version | undefined {
    const document = this.documents.get(id)
    return document?.versions.find(v => v.id === document.head)
  }

  /** The current profiles of the account, by name, as another computer would see them. */
  liveProfiles() {
    return [...this.documents.values()]
      .filter(d => d.kind === 'profile' && !this.head(d.id)!.deleted)
      .map(d => this.head(d.id)!.content.profile)
  }

  /** Another computer uploads a version of a document. */
  push(id: string, kind: Document['kind'], version: Omit<Version, 'id' | 'device'>, device = 'otro') {
    const created: Version = { ...version, id: randomUUID(), device }
    const document = this.documents.get(id)
    if (!document) {
      this.documents.set(id, { id, kind, head: created.id, versions: [created] })
      return { accepted: true, version: created }
    }
    document.versions.push(created)
    const head = this.head(id)!
    const accepted = created.parent === head.id || created.editedAt > head.editedAt
    if (accepted) {
      document.head = created.id
    }
    return { accepted, version: created }
  }

  private async handle(req: IncomingMessage): Promise<{ status: number, body?: unknown, headers?: Record<string, string> }> {
    const url = new URL(req.url ?? '/', this.url)
    const path = url.pathname

    if (req.method === 'GET' && path === '/auth/desktop') {
      const code = randomUUID()
      this.challenges.set(code, url.searchParams.get('challenge') ?? '')
      const back = `http://127.0.0.1:${url.searchParams.get('port')}/callback?code=${code}&state=${url.searchParams.get('state')}`
      return { status: 302, headers: { location: back } }
    }
    if (req.method === 'POST' && path === '/api/desktop/token') {
      const { code, verifier, device_name } = JSON.parse((await read(req)).toString())
      const challenge = this.challenges.get(code)
      this.challenges.delete(code)
      const expected = createHash('sha256').update(verifier).digest('base64url')
      if (!challenge || challenge !== expected) {
        return { status: 400, body: { statusMessage: 'Invalid code' } }
      }
      const token = randomUUID()
      const device = randomUUID()
      this.tokens.set(token, { device, name: device_name, revoked: false })
      return { status: 200, body: { token, device_id: device } }
    }

    const token = (req.headers.authorization ?? '').replace(/^Bearer /, '')
    const owner = this.tokens.get(token)
    if (!owner || owner.revoked) {
      return { status: 401, body: { statusMessage: 'Unauthorized' } }
    }
    if (req.method === 'DELETE' && path === '/api/desktop/token') {
      owner.revoked = true
      return { status: 204 }
    }
    if (req.method === 'GET' && path === '/api/sync/state') {
      return {
        status: 200,
        body: {
          documents: [...this.documents.values()].map((d) => {
            const head = this.head(d.id)!
            return { id: d.id, kind: d.kind, head: { version: head.id, edited_at: head.editedAt, deleted: head.deleted } }
          }),
        },
      }
    }
    const version = path.match(/^\/api\/sync\/documents\/([^/]+)\/versions(?:\/([^/]+))?$/)
    if (version && req.method === 'GET') {
      const found = this.documents.get(version[1]!)?.versions.find(v => v.id === version[2])
      return found ? { status: 200, body: { content: found.content } } : { status: 404 }
    }
    if (version && req.method === 'POST') {
      const body = JSON.parse((await read(req)).toString())
      if (this.full) {
        return { status: 507, body: { statusMessage: 'Storage quota exceeded' } }
      }
      const id = version[1]!
      const existing = this.documents.get(id)
      if (existing?.versions.some(v => v.id === body.version)) {
        const head = this.head(id)!
        return { status: 200, body: { accepted: head.id === body.version, head: { version: head.id, edited_at: head.editedAt, deleted: head.deleted } } }
      }
      const created: Version = { id: body.version, parent: body.parent, editedAt: body.edited_at, deleted: body.deleted, content: body.content, device: owner.name }
      let accepted = true
      if (existing) {
        existing.versions.push(created)
        const head = this.head(id)!
        accepted = created.parent === head.id || created.editedAt > head.editedAt
        if (accepted) {
          existing.head = created.id
        }
      }
      else {
        this.documents.set(id, { id, kind: body.kind, head: created.id, versions: [created] })
      }
      const head = this.head(id)!
      return { status: 200, body: { accepted, head: { version: head.id, edited_at: head.editedAt, deleted: head.deleted } } }
    }
    if (req.method === 'POST' && path === '/api/sync/assets/missing') {
      const { ids } = JSON.parse((await read(req)).toString())
      return { status: 200, body: { missing: (ids as string[]).filter(id => !this.assets.has(id)) } }
    }
    const asset = path.match(/^\/api\/sync\/assets\/([^/]+)$/)
    if (asset && req.method === 'PUT') {
      const bytes = await read(req)
      if (!asset[1]!.startsWith(createHash('sha256').update(bytes).digest('hex'))) {
        return { status: 400 }
      }
      this.assets.set(asset[1]!, bytes)
      return { status: 204 }
    }
    if (asset && req.method === 'GET') {
      const bytes = this.assets.get(asset[1]!)
      return bytes ? { status: 200, body: bytes, headers: { 'content-type': 'application/octet-stream' } } : { status: 404 }
    }
    return { status: 404 }
  }
}

async function read(req: IncomingMessage) {
  const chunks: Buffer[] = []
  for await (const chunk of req) {
    chunks.push(chunk as Buffer)
  }
  return Buffer.concat(chunks)
}
