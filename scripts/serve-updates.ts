// Publishes the last build as an update and serves it on http://127.0.0.1:8787, to test the
// updater end to end. Build the app with the local update configuration first:
//   bun scripts/build-app.ts --config src-tauri/tauri.local-updates.json
// Every build is newer than the one before, so an installed build updates to the next one.
import { copyFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { builtArtifacts, builtVersion, platformsOf, targetOf, updateManifest } from './update-manifest'

const PORT = 8787
const out = join(import.meta.dir, '..', '.updates')

const target = targetOf()
const version = await builtVersion(target)
const artifacts = (await builtArtifacts(target, version)).filter(artifact => artifact.updates.length)
await mkdir(out, { recursive: true })
for (const artifact of artifacts) {
  await copyFile(artifact.path, join(out, artifact.name))
}
const files = new Set(artifacts.map(artifact => artifact.name))
const platforms = platformsOf(artifacts, name => `http://127.0.0.1:${PORT}/${name}`)
const manifest = updateManifest(version, 'Local test build', platforms)
await Bun.write(join(out, 'latest.json'), JSON.stringify(manifest, null, 2))

Bun.serve({
  hostname: '127.0.0.1',
  port: PORT,
  fetch(request) {
    const name = new URL(request.url).pathname.slice(1)
    if (name !== 'latest.json' && !files.has(name)) {
      return new Response('not found', { status: 404 })
    }
    console.log(`${request.method} /${name}`)
    return new Response(Bun.file(join(out, name)))
  },
})
console.log(`serving version ${version} at http://127.0.0.1:${PORT}/latest.json`)
