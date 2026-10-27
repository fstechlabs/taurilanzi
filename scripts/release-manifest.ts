// Joins in .release/latest.json the parts release-assets.ts left for each system, once every
// build is in the folder.
// Usage: bun scripts/release-manifest.ts <version> <notes file>
import { readdir, rm } from 'node:fs/promises'
import { join } from 'node:path'
import type { Platforms } from './update-manifest'
import { updateManifest } from './update-manifest'

const out = join(import.meta.dir, '..', '.release')
const [version, notesFile] = process.argv.slice(2)
if (!version || !notesFile) {
  throw new Error('usage: bun scripts/release-manifest.ts <version> <notes file>')
}
const notes = (await Bun.file(notesFile).text()).trim()

const platforms: Platforms = {}
for (const name of (await readdir(out)).filter(file => /^platforms-.+\.json$/.test(file))) {
  Object.assign(platforms, await Bun.file(join(out, name)).json())
  await rm(join(out, name))
}
if (!Object.keys(platforms).length) {
  throw new Error(`no platforms in ${out}: gather the builds first`)
}
await Bun.write(join(out, 'latest.json'), JSON.stringify(updateManifest(version, notes, platforms), null, 2))
console.log(`latest.json offers ${version} to ${Object.keys(platforms).sort().join(', ')}`)
