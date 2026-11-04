// Puts the Python that runs plugins in src-tauri/resources/python, for the app to bundle: the
// standalone CPython pinned in python-runtime.json, from python-build-standalone, with the
// packages Ulanzi Studio's own Python carries, everything checked against its SHA-256. Tk, IDLE,
// the C headers and the shared library the interpreter does not use are left out.
// Usage: bun scripts/fetch-python.ts [--target darwin-arm64], targets as `${process.platform}-${process.arch}`
import { $ } from 'bun'
import { createHash } from 'node:crypto'
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { inflateRawSync } from 'node:zlib'
import runtime from './python-runtime.json'

const app = join(import.meta.dir, '..')
const destination = join(app, 'src-tauri', 'resources', 'python')
const flag = process.argv.indexOf('--target')
const target = flag > 0 ? process.argv[flag + 1]! : `${process.platform}-${process.arch}`
const archive = (runtime.archives as Record<string, { file: string, sha256: string }>)[target]
if (!archive) {
  console.error(`No Python pinned for ${target}`)
  process.exit(1)
}

const windows = target.startsWith('win32-')
const minor = runtime.version.split('.').slice(0, 2).join('.')
const binary = windows ? 'python.exe' : join('bin', 'python3')
const stamp = `${runtime.version}+${runtime.release} ${target} ${runtime.packages.map(p => p.file).join(' ')}`
const stampFile = join(destination, 'VERSION')
if (existsSync(join(destination, binary)) && existsSync(stampFile) && readFileSync(stampFile, 'utf8').trim() === stamp) {
  process.exit(0)
}

async function download(url: string, sha256: string) {
  const response = await fetch(url)
  if (!response.ok) {
    console.error(`Could not download ${url}: ${response.status}`)
    process.exit(1)
  }
  const bytes = Buffer.from(await response.arrayBuffer())
  const digest = createHash('sha256').update(bytes).digest('hex')
  if (digest !== sha256) {
    console.error(`${url} does not match its checksum: got ${digest}`)
    process.exit(1)
  }
  return bytes
}

/** A wheel is a ZIP file; Linux's tar cannot read those, so it is unpacked here. */
function unzip(bytes: Buffer, folder: string) {
  const end = bytes.lastIndexOf(Buffer.from([0x50, 0x4B, 0x05, 0x06]))
  const count = bytes.readUInt16LE(end + 10)
  let at = bytes.readUInt32LE(end + 16)
  for (let i = 0; i < count; i++) {
    const method = bytes.readUInt16LE(at + 10)
    const size = bytes.readUInt32LE(at + 20)
    const nameLength = bytes.readUInt16LE(at + 28)
    const extraLength = bytes.readUInt16LE(at + 30)
    const commentLength = bytes.readUInt16LE(at + 32)
    const local = bytes.readUInt32LE(at + 42)
    const name = bytes.toString('utf8', at + 46, at + 46 + nameLength)
    at += 46 + nameLength + extraLength + commentLength
    if (name.endsWith('/')) {
      continue
    }
    if (name.split('/').some(part => part === '..' || part === '') || name.startsWith('/')) {
      console.error(`Unsafe path in a wheel: ${name}`)
      process.exit(1)
    }
    const start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28)
    const data = bytes.subarray(start, start + size)
    const file = join(folder, name)
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, method === 8 ? inflateRawSync(data) : data)
  }
}

function remove(folder: string, names: string[]) {
  for (const name of names) {
    rmSync(join(folder, name), { recursive: true, force: true })
  }
}

function removeMatching(folder: string, test: (name: string) => boolean) {
  if (!existsSync(folder)) {
    return
  }
  for (const name of readdirSync(folder)) {
    if (test(name)) {
      rmSync(join(folder, name), { recursive: true, force: true })
    }
  }
}

console.log(`Downloading Python ${runtime.version} for ${target}…`)
const url = `https://github.com/astral-sh/python-build-standalone/releases/download/${runtime.release}/${encodeURIComponent(archive.file)}`
const bytes = await download(url, archive.sha256)

// Next to the destination, not in the system's temporary folder: a rename cannot cross file
// systems, and on some machines (GitHub's Ubuntu runners) that folder is one of its own.
mkdirSync(dirname(destination), { recursive: true })
const work = mkdtempSync(join(dirname(destination), '.python-download-'))
try {
  const file = join(work, 'python.tar.gz')
  writeFileSync(file, bytes)
  // Windows' own tar, unlike the GNU tar that Git Bash puts first in PATH.
  const tar = process.platform === 'win32' ? join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar'
  await $`${tar} -xzf ${file} -C ${work}`.quiet()
  const python = join(work, 'python')
  const lib = windows ? join(python, 'Lib') : join(python, 'lib', `python${minor}`)
  remove(lib, ['idlelib', 'tkinter', 'turtledemo', 'turtle.py'])
  removeMatching(lib, name => name.startsWith('config-'))
  if (windows) {
    remove(python, ['tcl', 'include', 'libs'])
    removeMatching(join(python, 'DLLs'), name => /^(_tkinter\.pyd|tcl.*\.dll|_test.*\.pyd|_ctypes_test\.pyd)$/.test(name))
  }
  else {
    remove(python, ['include', 'share'])
    removeMatching(join(python, 'lib'), name => /^(itcl|tcl|tk|thread|libtcl|pkgconfig|libpython)/.test(name))
    removeMatching(join(lib, 'lib-dynload'), name => name.startsWith('_tkinter') || name.startsWith('_test'))
    // The app copies links as whole files, so the interpreter is kept once, under the name the
    // engine looks for, and the pip launchers are pointed at it.
    const bin = join(python, 'bin')
    removeMatching(bin, name => lstatSync(join(bin, name)).isSymbolicLink() || /^(idle|pydoc|python.*-config)/.test(name))
    renameSync(join(bin, `python${minor}`), join(bin, 'python3'))
    for (const name of readdirSync(bin).filter(n => n.startsWith('pip'))) {
      const launcher = join(bin, name)
      writeFileSync(launcher, readFileSync(launcher, 'utf8').replaceAll(`/python${minor}"`, '/python3"'))
    }
  }
  const sitePackages = join(lib, 'site-packages')
  for (const wheel of runtime.packages) {
    unzip(await download(wheel.url, wheel.sha256), sitePackages)
  }
  rmSync(destination, { recursive: true, force: true })
  renameSync(python, destination)
  writeFileSync(stampFile, `${stamp}\n`)
  console.log(`Python ${runtime.version} ready in ${destination}`)
}
finally {
  rmSync(work, { recursive: true, force: true })
}
