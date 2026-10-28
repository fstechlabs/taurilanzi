import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { APP_BINARY } from './build'
import { CLOUD_PORT, COMMUNITY_PORT, shot } from './helpers'

// The launcher and each worker load this file; they share the folder through the environment.
const data = (process.env.TAURILANZI_E2E_DATA ??= mkdtempSync(join(tmpdir(), 'taurilanzi-e2e-')))

// `env` reaches the app but is missing from the published types, hence no object literal below.
const appOptions = {
  env: {
    TAURILANZI_DATA_DIR: data,
    TAURILANZI_NO_DOCK: '1',
    // The specs read the Spanish texts.
    TAURILANZI_LANGUAGE: 'es',
    TAURILANZI_COMMUNITY_CATALOG: `http://127.0.0.1:${COMMUNITY_PORT}/catalog.json`,
    TAURILANZI_CLOUD: `http://127.0.0.1:${CLOUD_PORT}`,
  },
  captureBackendLogs: true,
  backendLogLevel: 'warn' as const,
}

/** The real app, on its own data folder and without the dock, so the user's are left alone. */
export const config: WebdriverIO.Config = {
  runner: 'local',
  tsConfigPath: './tsconfig.json',
  specs: ['./specs/**/*.e2e.ts'],
  maxInstances: 1,
  capabilities: [{
    'browserName': 'tauri',
    'tauri:options': { application: APP_BINARY },
    'wdio:tauriServiceOptions': appOptions,
  } as WebdriverIO.Capabilities],
  services: [['@wdio/tauri-service', { driverProvider: 'embedded' }]],
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: { ui: 'bdd', timeout: 180_000, bail: true },
  logLevel: 'warn',
  waitforTimeout: 10_000,
  async afterTest(test, _context, { passed }) {
    if (!passed) {
      await shot(`failed-${test.title.replaceAll(/\W+/g, '-')}`)
    }
  },
  onComplete(exitCode: number) {
    if (exitCode === 0) {
      rmSync(data, { recursive: true, force: true })
    }
    else {
      console.log(`data kept in ${data}`)
    }
  },
}
