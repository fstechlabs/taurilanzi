// Builds the app for this system, with the Node and the Python that run plugins, and signs its
// update files:
// the .app and a .dmg on macOS (the .dmg from scripts/build-dmg.py, through uv), the NSIS
// installer on Windows, and an AppImage, a .deb and an .rpm on Linux. Bundle targets per system
// are in src-tauri/tauri.<system>.conf.json.
// Releases are signed in GitHub Actions, with the key in TAURI_SIGNING_PRIVATE_KEY. Without it,
// the build is signed with a test key of this computer's own, created on the first build, and
// trusts that key instead: enough to try the updater locally, useless to anyone else.
// On macOS, releases are also signed with the project's Developer ID and notarized (see
// macos-signing.ts); builds by hand without its certificate are signed ad hoc.
// The version is the time of the build (see release-version.ts), or TAURILANZI_VERSION if set.
// Usage: bun scripts/build-app.ts [--target <rust target>] [extra tauri build arguments, e.g.
// --config file.json]
import { $ } from 'bun'
import { existsSync, mkdirSync, rmSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { checkNotarized, openSigning, type MacSigning, signAndNotarize, signingRequested, signRuntimes } from './macos-signing'
import { releaseVersion } from './release-version'
import { bundleDir, nodeTarget, targetOf } from './update-manifest'

const ROOT = join(import.meta.dir, '..')
const TEST_KEY = join(homedir(), '.tauri', 'taurilanzi-test.key')
const TEST_PASSWORD = 'local-test'

const args = process.argv.slice(2)
const flag = args.indexOf('--target')
const target = targetOf(flag >= 0 ? args[flag + 1] : undefined)
const host = targetOf()

const version = process.env.TAURILANZI_VERSION || releaseVersion()
console.log(`Building Taurilanzi ${version} for ${target.os} ${target.arch}`)

const config: Record<string, unknown> = { version }
let key = process.env.TAURI_SIGNING_PRIVATE_KEY
let password = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD
if (!key) {
  if (!existsSync(TEST_KEY)) {
    mkdirSync(join(homedir(), '.tauri'), { recursive: true })
    await $`bun run tauri signer generate --ci -p ${TEST_PASSWORD} -w ${TEST_KEY}`.quiet()
  }
  console.log(`Signing with the local test key, ${TEST_KEY}: this build only takes updates signed with it`)
  key = TEST_KEY
  password = TEST_PASSWORD
  config.plugins = { updater: { pubkey: (await Bun.file(`${TEST_KEY}.pub`).text()).trim() } }
}

/** The Node and the Python that run plugins, both named by target as fetch-node.ts names it. */
async function fetchRuntimes(runtime: string) {
  for (const script of ['fetch-node.ts', 'fetch-python.ts']) {
    const fetched = await $`bun scripts/${script} --target ${runtime}`.cwd(ROOT).nothrow()
    if (fetched.exitCode !== 0) {
      process.exit(fetched.exitCode)
    }
  }
}

await fetchRuntimes(nodeTarget(target))

const env: Record<string, string | undefined> = {
  ...process.env,
  // The macOS bundler needs Apple's xattr; others (e.g. from Python) may come first in PATH.
  PATH: target.os === 'darwin' ? `/usr/bin:${process.env.PATH}` : process.env.PATH!,
  // For the engine, which reads it when compiled (src-tauri/src/version.rs).
  TAURILANZI_VERSION: version,
  TAURI_SIGNING_PRIVATE_KEY: key,
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password,
}

let signing: MacSigning | undefined
if (target.os === 'darwin') {
  if (signingRequested()) {
    signing = await openSigning()
    try {
      await signRuntimes(signing, ['node', 'python'].map(dir => join(ROOT, 'src-tauri', 'resources', dir)))
    }
    catch (error) {
      await signing.close()
      throw error
    }
    config.bundle = { macOS: { signingIdentity: signing.identity } }
    Object.assign(env, signing.env)
    // Tauri would import the certificate again, and only takes it under the identity it is given.
    delete env.APPLE_CERTIFICATE
    delete env.APPLE_CERTIFICATE_PATH
    delete env.APPLE_CERTIFICATE_PASSWORD
    delete env.APPLE_API_PRIVATE_KEY
  }
  else if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
    console.error('A release for macOS needs the Developer ID certificate in APPLE_CERTIFICATE')
    process.exit(1)
  }
}

// Only release builds carry Node and Python: dev builds and tests find them in the source tree.
const build = await $`bun run tauri build --config src-tauri/tauri.bundle.json --config ${JSON.stringify(config)} ${args}`
  .cwd(ROOT)
  .env(env)
  .nothrow()

// Development builds and tests run plugins with the Node and the Python of this computer again.
if (nodeTarget(target) !== nodeTarget(host)) {
  await fetchRuntimes(nodeTarget(host))
}
if (build.exitCode !== 0) {
  await signing?.close()
  process.exit(build.exitCode)
}

if (target.os === 'darwin') {
  try {
    const bundle = bundleDir(target)
    const dmgFolder = join(bundle, 'dmg')
    rmSync(dmgFolder, { recursive: true, force: true })
    mkdirSync(dmgFolder)
    const app = join(bundle, 'macos', 'Taurilanzi.app')
    const dmg = join(dmgFolder, `Taurilanzi_${version}_${target.arch}.dmg`)
    const packed = await $`uv run scripts/build-dmg.py ${app} ${dmg} src-tauri/icons/icon.icns`
      .cwd(ROOT)
      .nothrow()
    if (packed.exitCode !== 0) {
      process.exitCode = packed.exitCode
    }
    else if (signing) {
      await checkNotarized(app)
      await signAndNotarize(signing, dmg)
      await checkNotarized(dmg)
    }
  }
  finally {
    await signing?.close()
  }
}
