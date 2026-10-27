// Builds the app with the embedded WebDriver server and runs the end-to-end tests against it.
// Usage: bun tests/e2e/run.ts [--skip-build] [--release-name] [extra wdio arguments, e.g. --spec specs/canvas.e2e.ts]
// --release-name builds it with the released app's name instead of the development one, for the
// README's screenshots.
import { $, Glob } from 'bun'
import { join } from 'node:path'
import { TARGET_DIR } from './build'

const app = join(import.meta.dir, '..', '..')
const args = process.argv.slice(2)
const skipBuild = args.includes('--skip-build')
const releaseName = args.includes('--release-name')
const wdioArgs = args.filter(a => a !== '--skip-build' && a !== '--release-name')

if (!skipBuild) {
  await $`bun run tauri build --debug --no-bundle --features e2e --config src-tauri/tauri.e2e.json ${releaseName ? ['--config', JSON.stringify({ productName: 'Taurilanzi' })] : []}`
    .cwd(app)
    .env({ ...process.env, CARGO_TARGET_DIR: TARGET_DIR, VITE_E2E: '1' })
}

// The service starts one app for a whole wdio run, so each spec gets its own run: a fresh app on
// a fresh data folder, with nothing left over from the specs before it.
const chosen = wdioArgs.some(a => a === '--spec' || a.startsWith('--spec='))
const runs = chosen
  ? [wdioArgs]
  : [...new Glob('specs/*.e2e.ts').scanSync(import.meta.dir)].sort().map(spec => ['--spec', spec, ...wdioArgs])

const failed: string[] = []
for (const runArgs of runs) {
  const { TAURILANZI_E2E_DATA: _, ...env } = process.env
  const run = await $`bun x wdio run wdio.conf.ts ${runArgs}`.cwd(import.meta.dir).env(env).nothrow()
  if (run.exitCode !== 0) {
    failed.push(chosen ? 'the chosen specs' : runArgs[1]!)
  }
}
if (failed.length > 0) {
  console.log(`\nFailed: ${failed.join(', ')}`)
}
process.exit(failed.length > 0 ? 1 : 0)
