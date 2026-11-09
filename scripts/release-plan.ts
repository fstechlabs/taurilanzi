// Decides whether the checked-out commit gets a release, and with which version and notes.
// It does when a commit since the last release changes the app: a feat, fix, perf, refactor or
// build, or anything marked as breaking. FORCE=true releases anyway.
// In GitHub Actions it writes its answer to GITHUB_OUTPUT, and to RUNNER_TEMP the notes of the
// release and the shorter ones of latest.json.
// Usage: bun scripts/release-plan.ts
import { $ } from 'bun'
import { appendFileSync } from 'node:fs'
import { join } from 'node:path'
import { changesOf, commitsSince, releaseNotes, releases, shortNotes, withAuthors } from './release-notes'
import { isNewer, releaseDate, releaseVersion } from './release-version'

const REPO = process.env.GITHUB_REPOSITORY ?? 'fstechlabs/taurilanzi'
const TAGS = 'v[0-9]*'

const head = (await $`git rev-parse HEAD`.text()).trim()
const last = (await $`git describe --tags --abbrev=0 --match ${TAGS}`.nothrow().quiet().text()).trim()
const released = last !== '' && (await $`git rev-list -n 1 ${last}`.text()).trim() === head

const changes = changesOf(await commitsSince(last))

const version = releaseVersion()
if (last !== '' && !isNewer(version, last.slice(1))) {
  console.error(`${version} is not newer than the last release, ${last}`)
  process.exit(1)
}

const release = !released && (releases(changes) || process.env.FORCE === 'true')
const reason = released
  ? `${head.slice(0, 7)} is already released as ${last}`
  : release ? `releasing ${version}` : `nothing since ${last || 'the start'} changes the app`
console.log(reason)

const tag = `v${version}`
const notes = releaseNotes(await withAuthors(changes, REPO), { repo: REPO, last, tag })
const updateNotes = shortNotes(changes, last)
const outputs = { release, version, tag, title: `${version} · ${releaseDate(version)}` }
if (process.env.GITHUB_OUTPUT) {
  const files = { notes: join(process.env.RUNNER_TEMP!, 'release-notes.md'), update_notes: join(process.env.RUNNER_TEMP!, 'update-notes.txt') }
  await Bun.write(files.notes, `${notes}\n`)
  await Bun.write(files.update_notes, `${updateNotes}\n`)
  appendFileSync(process.env.GITHUB_OUTPUT, Object.entries({ ...outputs, ...files }).map(([k, v]) => `${k}=${v}\n`).join(''))
}
else {
  console.log(outputs)
  console.log(`\n${notes}\n\n--- latest.json ---\n${updateNotes}`)
}
