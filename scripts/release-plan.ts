// Decides whether the checked-out commit gets a release, and with which version and notes.
// It does when a commit since the last release changes the app: a feat, fix, perf, refactor or
// build, or anything marked as breaking. FORCE=true releases anyway.
// In GitHub Actions it writes its answer to GITHUB_OUTPUT and the notes to RUNNER_TEMP.
// Usage: bun scripts/release-plan.ts
import { $ } from 'bun'
import { appendFileSync } from 'node:fs'
import { join } from 'node:path'
import { commitType } from './check-commits'
import { isNewer, releaseDate, releaseVersion } from './release-version'

const REPO = process.env.GITHUB_REPOSITORY ?? 'fstechlabs/taurilanzi'
const RELEASED_BY = ['feat', 'fix', 'perf', 'refactor', 'build']
const TAGS = 'v[0-9]*'

const head = (await $`git rev-parse HEAD`.text()).trim()
const last = (await $`git describe --tags --abbrev=0 --match ${TAGS}`.nothrow().quiet().text()).trim()
const released = last !== '' && (await $`git rev-list -n 1 ${last}`.text()).trim() === head

const range = last ? `${last}..HEAD` : 'HEAD'
const subjects = (await $`git log --format=%s ${range}`.text()).split('\n').filter(Boolean)
const changes = subjects.filter((subject) => {
  const commit = commitType(subject)
  return commit !== null && (RELEASED_BY.includes(commit.type) || commit.breaking)
})

const version = releaseVersion()
if (last !== '' && !isNewer(version, last.slice(1))) {
  console.error(`${version} is not newer than the last release, ${last}`)
  process.exit(1)
}

const release = !released && (changes.length > 0 || process.env.FORCE === 'true')
const reason = released
  ? `${head.slice(0, 7)} is already released as ${last}`
  : release ? `releasing ${version}` : `nothing since ${last || 'the start'} changes the app`
console.log(reason)

const notes = last
  ? `Every change since ${last}: https://github.com/${REPO}/compare/${last}...v${version}`
  : 'The first release of Taurilanzi.'
const outputs = { release, version, tag: `v${version}`, title: `${version} · ${releaseDate(version)}` }
if (process.env.GITHUB_OUTPUT) {
  const file = join(process.env.RUNNER_TEMP!, 'release-notes.md')
  await Bun.write(file, `${notes}\n`)
  appendFileSync(process.env.GITHUB_OUTPUT, Object.entries({ ...outputs, notes: file }).map(([k, v]) => `${k}=${v}\n`).join(''))
}
else {
  console.log(outputs)
  console.log(`\n${notes}`)
}
