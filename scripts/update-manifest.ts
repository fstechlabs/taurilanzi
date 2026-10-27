// What a build leaves in the bundle folder for each system: the installers to publish, the signed
// files the updater takes, and the latest.json that offers them.
import { $ } from 'bun'
import { existsSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

export type Os = 'darwin' | 'windows' | 'linux'
export type Arch = 'aarch64' | 'x86_64'

export interface Target {
  os: Os
  arch: Arch
  /** The Rust target, when not the computer's own. */
  triple?: string
}

/** A file of the release: `updates` lists the updater platforms it serves, if any. */
export interface Artifact {
  path: string
  name: string
  updates: string[]
  signature?: string
}

const ROOT = join(import.meta.dir, '..')

/** The system and processor a Rust target builds for, or this computer's. */
export function targetOf(triple?: string): Target {
  if (triple) {
    const arch: Arch = triple.startsWith('aarch64') ? 'aarch64' : 'x86_64'
    const os: Os = triple.includes('apple') ? 'darwin' : triple.includes('windows') ? 'windows' : 'linux'
    return { os, arch, triple }
  }
  const os: Os = process.platform === 'win32' ? 'windows' : process.platform === 'darwin' ? 'darwin' : 'linux'
  return { os, arch: process.arch === 'arm64' ? 'aarch64' : 'x86_64' }
}

/** How scripts/node-runtime.json names the Node of a target. */
export function nodeTarget(target: Target) {
  const platform = { darwin: 'darwin', windows: 'win32', linux: 'linux' }[target.os]
  return `${platform}-${target.arch === 'aarch64' ? 'arm64' : 'x64'}`
}

export function bundleDir(target: Target) {
  const release = target.triple ? join('target', target.triple, 'release') : join('target', 'release')
  return join(ROOT, 'src-tauri', release, 'bundle')
}

function find(folder: string, suffix: string) {
  if (!existsSync(folder)) {
    return null
  }
  const name = readdirSync(folder).find(file => file.endsWith(suffix))
  return name ? join(folder, name) : null
}

/** The version the last build has, from its bundle. */
export async function builtVersion(target: Target): Promise<string> {
  const bundle = bundleDir(target)
  if (target.os === 'darwin') {
    const plist = join(bundle, 'macos', 'Taurilanzi.app', 'Contents', 'Info.plist')
    return (await $`plutil -extract CFBundleShortVersionString raw ${plist}`.text()).trim()
  }
  const installer = target.os === 'windows' ? find(join(bundle, 'nsis'), '.exe') : find(join(bundle, 'appimage'), '.AppImage')
  const version = installer?.match(/_(\d+\.\d+\.\d+)_/)?.[1]
  if (!version) {
    throw new Error(`no installer in ${bundle}: build the app first`)
  }
  return version
}

/** Every file of the build, named `Taurilanzi_<version>_<arch>.<kind>` so the systems never clash. */
export async function builtArtifacts(target: Target, version: string): Promise<Artifact[]> {
  const bundle = bundleDir(target)
  const platform = `${target.os}-${target.arch}`
  const name = (ext: string) => `Taurilanzi_${version}_${target.arch}.${ext}`
  // Packages update themselves only with a bundler that signs them: until then they are plain
  // downloads, and the AppImage is what updates on Linux.
  const wanted: { folder: string, suffix: string, ext: string, updates: string[], optional?: boolean }[] = {
    darwin: [
      { folder: 'dmg', suffix: '.dmg', ext: 'dmg', updates: [] },
      { folder: 'macos', suffix: '.app.tar.gz', ext: 'app.tar.gz', updates: [platform, `${platform}-app`] },
    ],
    windows: [
      { folder: 'nsis', suffix: '.exe', ext: 'setup.exe', updates: [platform, `${platform}-nsis`] },
    ],
    linux: [
      { folder: 'appimage', suffix: '.AppImage', ext: 'AppImage', updates: [platform, `${platform}-appimage`] },
      { folder: 'deb', suffix: '.deb', ext: 'deb', updates: [`${platform}-deb`], optional: true },
      { folder: 'rpm', suffix: '.rpm', ext: 'rpm', updates: [`${platform}-rpm`], optional: true },
    ],
  }[target.os]
  const artifacts: Artifact[] = []
  for (const file of wanted) {
    const path = find(join(bundle, file.folder), file.suffix)
    if (!path) {
      throw new Error(`no ${file.suffix} in ${join(bundle, file.folder)}: build the app first`)
    }
    const signed = existsSync(`${path}.sig`)
    if (file.updates.length && !signed && !file.optional) {
      throw new Error(`${path} is not signed`)
    }
    artifacts.push({
      path,
      name: name(file.ext),
      updates: signed ? file.updates : [],
      signature: signed ? (await Bun.file(`${path}.sig`).text()).trim() : undefined,
    })
  }
  return artifacts
}

export type Platforms = Record<string, { signature: string, url: string }>

/** The updater platforms the artifacts serve, each with its file at `urlOf(name)`. */
export function platformsOf(artifacts: Artifact[], urlOf: (name: string) => string): Platforms {
  const platforms: Platforms = {}
  for (const artifact of artifacts) {
    for (const platform of artifact.updates) {
      platforms[platform] = { signature: artifact.signature!, url: urlOf(artifact.name) }
    }
  }
  return platforms
}

export function updateManifest(version: string, notes: string, platforms: Platforms) {
  return {
    version,
    notes,
    pub_date: new Date().toISOString(),
    platforms,
  }
}
