// Signs and notarizes the macOS app with the project's Developer ID. macOS ties the Accessibility
// permission to the signature: an ad hoc one changes with every build, so each update lost it,
// while a Developer ID one holds it across updates (and certificate renewals: it follows the team).
// The certificate goes into a keychain of its own for the build. Tauri signs and notarizes the app
// with it; this signs what Tauri leaves alone, which notarization checks all the same: the Node and
// the Python in the app's resources, and the .dmg, notarized here too.
// Environment: APPLE_CERTIFICATE (a base64 .p12, or APPLE_CERTIFICATE_PATH with the file) and
// APPLE_CERTIFICATE_PASSWORD; for notarizing, an App Store Connect API key: APPLE_API_ISSUER,
// APPLE_API_KEY (its ID) and APPLE_API_KEY_PATH, or APPLE_API_PRIVATE_KEY with the key itself.
// For a local test, in an env file outside the repository: bun --env-file=<file> scripts/build-app.ts
import { $ } from 'bun'
import { randomBytes } from 'node:crypto'
import { closeSync, lstatSync, mkdtempSync, openSync, readdirSync, readFileSync, readSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const ROOT = join(import.meta.dir, '..')
// Node's own entitlements without get-task-allow, which notarization refuses; Python needs to load
// the native modules plugins bring, signed by nobody or ad hoc.
const RUNTIME_ENTITLEMENTS = join(ROOT, 'src-tauri', 'entitlements', 'plugin-runtimes.plist')
const INTERMEDIATE = 'https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer'

export interface MacSigning {
  /** The certificate's SHA-1, which codesign takes as the identity. */
  identity: string
  keychain: string
  /** What Tauri needs to notarize the app. */
  env: Record<string, string>
  close(): Promise<void>
}

export function signingRequested() {
  return Boolean(process.env.APPLE_CERTIFICATE || process.env.APPLE_CERTIFICATE_PATH)
}

export async function openSigning(): Promise<MacSigning> {
  const { APPLE_CERTIFICATE_PATH, APPLE_CERTIFICATE_PASSWORD, APPLE_API_ISSUER, APPLE_API_KEY } = process.env
  const certificate = APPLE_CERTIFICATE_PATH
    ? readFileSync(APPLE_CERTIFICATE_PATH)
    : Buffer.from(process.env.APPLE_CERTIFICATE ?? '', 'base64')
  if (!certificate.length || !APPLE_API_ISSUER || !APPLE_API_KEY
    || !(process.env.APPLE_API_KEY_PATH || process.env.APPLE_API_PRIVATE_KEY)) {
    throw new Error('Signing for macOS needs APPLE_CERTIFICATE, APPLE_API_ISSUER, APPLE_API_KEY and APPLE_API_KEY_PATH or APPLE_API_PRIVATE_KEY')
  }

  const dir = mkdtempSync(join(tmpdir(), 'taurilanzi-signing-'))
  const keychain = join(dir, 'signing.keychain-db')
  const secret = randomBytes(24).toString('hex')
  const previous = (await $`security list-keychains -d user`.text())
    .split('\n')
    .map(line => line.trim().replace(/^"|"$/g, ''))
    .filter(Boolean)

  const close = async () => {
    await $`security list-keychains -d user -s ${previous}`.quiet().nothrow()
    await $`security delete-keychain ${keychain}`.quiet().nothrow()
    rmSync(dir, { recursive: true, force: true })
  }

  try {
    const p12 = join(dir, 'certificate.p12')
    writeFileSync(p12, certificate, { mode: 0o600 })
    const intermediate = join(dir, 'intermediate.cer')
    writeFileSync(intermediate, Buffer.from(await (await fetch(INTERMEDIATE)).arrayBuffer()))

    await $`security create-keychain -p ${secret} ${keychain}`.quiet()
    // Without a timeout, so it does not lock in the middle of a long build.
    await $`security set-keychain-settings ${keychain}`.quiet()
    await $`security unlock-keychain -p ${secret} ${keychain}`.quiet()
    await $`security import ${p12} -k ${keychain} -P ${APPLE_CERTIFICATE_PASSWORD ?? ''} -T /usr/bin/codesign`.quiet()
    await $`security import ${intermediate} -k ${keychain}`.quiet().nothrow()
    await $`security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k ${secret} ${keychain}`.quiet()
    await $`security list-keychains -d user -s ${previous} ${keychain}`.quiet()
    rmSync(p12)

    const identities = await $`security find-identity -v -p codesigning ${keychain}`.text()
    const identity = identities.match(/\b[0-9A-F]{40}\b(?= "Developer ID Application:)/)?.[0]
    if (!identity) {
      throw new Error(`APPLE_CERTIFICATE holds no valid Developer ID Application identity:\n${identities}`)
    }

    let keyPath = process.env.APPLE_API_KEY_PATH
    if (!keyPath) {
      keyPath = join(dir, `AuthKey_${APPLE_API_KEY}.p8`)
      writeFileSync(keyPath, process.env.APPLE_API_PRIVATE_KEY!, { mode: 0o600 })
    }
    const env = { APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH: keyPath }
    return { identity, keychain, env, close }
  }
  catch (error) {
    await close()
    throw error
  }
}

function isMachO(path: string) {
  const bytes = Buffer.alloc(4)
  const fd = openSync(path, 'r')
  try {
    if (readSync(fd, bytes, 0, 4, 0) < 4) {
      return false
    }
  }
  finally {
    closeSync(fd)
  }
  return [0xFEEDFACF, 0xCFFAEDFE, 0xFEEDFACE, 0xCEFAEDFE, 0xCAFEBABE].includes(bytes.readUInt32BE(0))
}

function machOFiles(dir: string): string[] {
  return readdirSync(dir, { recursive: true, encoding: 'utf8' })
    .map(name => join(dir, name))
    .filter(path => lstatSync(path).isFile() && isMachO(path))
}

/** Signs every program and library in the folders, libraries first, as codesign wants. */
export async function signRuntimes(signing: MacSigning, dirs: string[]) {
  const files = dirs.flatMap(machOFiles)
  const library = (path: string) => /\.(so|dylib)$/.test(path)
  for (const file of [...files.filter(library), ...files.filter(path => !library(path))]) {
    await $`codesign --force --sign ${signing.identity} --keychain ${signing.keychain} --options runtime --timestamp --entitlements ${RUNTIME_ENTITLEMENTS} ${file}`.quiet()
  }
  console.log(`Signed ${files.length} programs and libraries of the plugin runtimes`)
}

export async function signAndNotarize(signing: MacSigning, file: string) {
  await $`codesign --force --sign ${signing.identity} --keychain ${signing.keychain} --timestamp ${file}`.quiet()
  const { APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH } = signing.env
  const credentials = ['--key', APPLE_API_KEY_PATH!, '--key-id', APPLE_API_KEY!, '--issuer', APPLE_API_ISSUER!]
  console.log(`Notarizing ${file}…`)
  const submitted = await $`xcrun notarytool submit ${file} ${credentials} --wait --output-format json`.nothrow().quiet()
  let result: { id?: string, status?: string } = {}
  try {
    result = JSON.parse(submitted.stdout.toString())
  }
  catch {}
  if (result.status !== 'Accepted') {
    if (result.id) {
      await $`xcrun notarytool log ${result.id} ${credentials}`.nothrow()
    }
    throw new Error(`Notarization of ${file} failed: ${result.status ?? submitted.stderr.toString()}`)
  }
  await $`xcrun stapler staple ${file}`.quiet()
}

/** Fails unless Gatekeeper takes the app or the .dmg as notarized Developer ID code. */
export async function checkNotarized(path: string) {
  const type = path.endsWith('.dmg') ? ['-t', 'open', '--context', 'context:primary-signature'] : ['-t', 'exec']
  const assessed = await $`spctl --assess ${type} -vv ${path}`.quiet().nothrow()
  const text = `${assessed.stdout}${assessed.stderr}`
  if (assessed.exitCode !== 0 || !text.includes('Notarized Developer ID')) {
    throw new Error(`Gatekeeper does not take ${path} as notarized:\n${text}`)
  }
}
