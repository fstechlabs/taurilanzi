import type { Commit } from '../../scripts/release-notes'
import { expect, test } from 'bun:test'
import { changesOf, releaseNotes, releases, shortNotes, unwrap } from '../../scripts/release-notes'

const REPO = 'owner/repo'

function commit(hash: string, subject: string, body = ''): Commit {
  return { hash: hash.padEnd(40, '0'), author: 'Ana', subject, body }
}

test('only the commits that change the app are listed', () => {
  const changes = changesOf([
    commit('a1', 'feat(editor): paint keys'),
    commit('b2', 'docs(readme): explain keys'),
    commit('c3', 'fix: resend dropped keys'),
    commit('d4', 'ci(tests): run on Arm'),
    commit('e5', 'build(release): sign the app'),
    commit('f6', 'not conventional at all'),
  ])
  expect(changes.map(c => c.type)).toEqual(['feat', 'fix', 'build'])
})

test('a breaking change is marked by its ! or by its footer, whatever its type', () => {
  const changes = changesOf([
    commit('a1', 'feat(cloud)!: new sync format'),
    commit('b2', 'docs: move the data folder', 'The folder moved.\n\nBREAKING CHANGE: data lives elsewhere'),
    commit('c3', 'feat: plain'),
  ])
  expect(changes.map(c => c.breaking)).toEqual([true, true, false])
})

test('a commit reverted in the same release is left out with its revert', () => {
  const changes = changesOf([
    commit('a1', 'feat(ui): a thing'),
    commit('b2', 'Revert "feat(ui): a thing"', `This reverts commit ${'a1'.padEnd(40, '0')}.`),
    commit('c3', 'revert: an older feature', 'This reverts commit 9999999.'),
  ])
  expect(changes.map(c => c.hash.slice(0, 2))).toEqual(['c3'])
  expect(releases(changes)).toBe(false)
})

test('hard-wrapped bodies are joined, keeping paragraphs and lists and dropping trailers', () => {
  const body = 'A long line\nwrapped here.\n\n- one item\n  that wraps\n- two\n\nCo-authored-by: Bea <bea@example.com>'
  expect(unwrap(body)).toBe('A long line wrapped here.\n\n- one item that wraps\n- two')
})

test('the notes group the changes, link each commit and name its authors', () => {
  const changes = changesOf([
    commit('a1', 'fix(hass): keep tiles <live>', 'Tiles\nstay live.'),
    commit('b2', 'feat(plugins): run Python plugins'),
    commit('c3', 'refactor!: drop the old store'),
  ]).map(c => ({ ...c, authors: ['@ana'] }))
  const notes = releaseNotes(changes, { repo: REPO, last: 'v1', tag: 'v2' })
  expect(notes.indexOf('## ⚠️ Breaking changes')).toBeLessThan(notes.indexOf('## New'))
  expect(notes.indexOf('## New')).toBeLessThan(notes.indexOf('## Fixes'))
  expect(notes).not.toContain('## Under the hood')
  expect(notes).toContain(`- **Home Assistant:** Keep tiles &lt;live&gt; ([\`a100000\`](https://github.com/${REPO}/commit/${'a1'.padEnd(40, '0')}), by @ana)`)
  expect(notes).toContain('  <details><summary>Details</summary>\n\n  Tiles stay live.\n\n  </details>')
  expect(notes).toContain(`https://github.com/${REPO}/compare/v1...v2`)
})

test('the notes of latest.json are the subjects in plain text', () => {
  const changes = changesOf([commit('a1', 'feat(editor): paint keys', 'Long body.'), commit('b2', 'perf: draw faster')])
  expect(shortNotes(changes, 'v1')).toBe('New:\n- Editor: Paint keys\n\nPerformance:\n- Draw faster')
})

test('a release with nothing that changes the app says so', () => {
  expect(releaseNotes([], { repo: REPO, last: 'v1', tag: 'v2' })).toStartWith('Nothing in the app changed since v1.')
  expect(shortNotes([], 'v1')).toBe('Nothing in the app changed since v1.')
})
