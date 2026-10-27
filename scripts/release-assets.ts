// Gathers in .release/ what one build publishes, after build-app.ts: its installers, its signed
// update files, and the part of latest.json that offers them (scripts/release-manifest.ts joins
// the parts of every system).
// Usage: bun scripts/release-assets.ts [--target <rust target>]
import { copyFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { builtArtifacts, builtVersion, platformsOf, targetOf } from './update-manifest'

const REPO = process.env.GITHUB_REPOSITORY ?? 'fstechlabs/taurilanzi'
const out = join(import.meta.dir, '..', '.release')

const flag = process.argv.indexOf('--target')
const target = targetOf(flag > 0 ? process.argv[flag + 1] : undefined)
const version = process.env.TAURILANZI_VERSION || await builtVersion(target)
const artifacts = await builtArtifacts(target, version)
await mkdir(out, { recursive: true })
for (const artifact of artifacts) {
  await copyFile(artifact.path, join(out, artifact.name))
  if (artifact.signature) {
    await Bun.write(join(out, `${artifact.name}.sig`), artifact.signature)
  }
}

const platforms = platformsOf(artifacts, name => `https://github.com/${REPO}/releases/download/v${version}/${name}`)
await Bun.write(join(out, `platforms-${target.os}-${target.arch}.json`), JSON.stringify(platforms, null, 2))
console.log(`${target.os} ${target.arch} files of ${version} ready in ${out}`)
