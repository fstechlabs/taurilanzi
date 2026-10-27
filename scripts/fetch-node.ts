// Puts the Node that runs plugins in src-tauri/resources/node, for the app to bundle: the
// version pinned in node-runtime.json, from nodejs.org, checked against its SHA-256.
// Usage: bun scripts/fetch-node.ts [--target darwin-arm64], targets as `${process.platform}-${process.arch}`
import { $ } from 'bun'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import runtime from './node-runtime.json'

const app = join(import.meta.dir, '..')
const destination = join(app, 'src-tauri', 'resources', 'node')
const flag = process.argv.indexOf('--target')
const target = flag > 0 ? process.argv[flag + 1]! : `${process.platform}-${process.arch}`
const archive = (runtime.archives as Record<string, { file: string, sha256: string }>)[target]
if (!archive) {
  console.error(`No Node pinned for ${target}`)
  process.exit(1)
}

const windows = target.startsWith('win32-')
const binary = windows ? 'node.exe' : 'node'
const stamp = `${runtime.version} ${target}`
const stampFile = join(destination, 'VERSION')
if (existsSync(join(destination, binary)) && existsSync(stampFile) && readFileSync(stampFile, 'utf8').trim() === stamp) {
  process.exit(0)
}

const url = `https://nodejs.org/dist/v${runtime.version}/${archive.file}`
console.log(`Downloading Node ${runtime.version} for ${target}…`)
const response = await fetch(url)
if (!response.ok) {
  console.error(`Could not download ${url}: ${response.status}`)
  process.exit(1)
}
const bytes = Buffer.from(await response.arrayBuffer())
const digest = createHash('sha256').update(bytes).digest('hex')
if (digest !== archive.sha256) {
  console.error(`${archive.file} does not match its checksum: got ${digest}`)
  process.exit(1)
}

// Next to the destination, not in the system's temporary folder: a rename cannot cross file
// systems, and on some machines (GitHub's Ubuntu runners) that folder is one of its own.
mkdirSync(dirname(destination), { recursive: true })
const work = mkdtempSync(join(dirname(destination), '.node-download-'))
try {
  const file = join(work, archive.file)
  writeFileSync(file, bytes)
  const folder = archive.file.replace(/\.(tar\.xz|zip)$/, '')
  // Windows builds come zipped, with the program at the top; Windows' own tar reads ZIP files,
  // unlike the GNU tar that Git Bash puts first in PATH.
  const program = windows ? `${folder}/node.exe` : `${folder}/bin/node`
  const tar = windows ? join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar'
  await $`${tar} -xf ${file} -C ${work} ${program} ${folder}/LICENSE`.quiet()
  rmSync(destination, { recursive: true, force: true })
  mkdirSync(destination, { recursive: true })
  renameSync(join(work, program), join(destination, binary))
  renameSync(join(work, folder, 'LICENSE'), join(destination, 'LICENSE'))
  writeFileSync(stampFile, `${stamp}\n`)
  console.log(`Node ${runtime.version} ready in ${destination}`)
}
finally {
  rmSync(work, { recursive: true, force: true })
}
