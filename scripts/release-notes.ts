// The notes of a release, from the commits since the one before: those that change the app,
// grouped by type, each with its author and its whole message.
import { $ } from 'bun'
import { commitType } from './check-commits'

export interface Commit {
  hash: string
  author: string
  subject: string
  body: string
}

export interface Change extends Commit {
  type: string
  scope: string | null
  description: string
  breaking: boolean
  /** Co-authors from the trailers, and the GitHub login of the author when known. */
  authors: string[]
}

const SECTIONS: { title: string, types: string[] }[] = [
  { title: 'New', types: ['feat'] },
  { title: 'Fixes', types: ['fix'] },
  { title: 'Performance', types: ['perf'] },
  { title: 'Under the hood', types: ['refactor', 'build'] },
  { title: 'Reverted', types: ['revert'] },
]
const RELEASED_BY = ['feat', 'fix', 'perf', 'refactor', 'build']
const SCOPES: Record<string, string> = { hass: 'Home Assistant', obs: 'OBS', ui: 'UI', ipc: 'IPC', ci: 'CI' }
const BREAKING = /^BREAKING[ -]CHANGE: /m
const REVERTS = /This reverts commit ([0-9a-f]{7,40})/
const TRAILER = /^(Co-authored-by|Signed-off-by|Reviewed-by|Helped-by): (.+?)(?: <.*>)?$/i
const FIELD = '\x1F'
const RECORD = '\x1E'

/** The commits after `from` up to HEAD, oldest first. */
export async function commitsSince(from: string): Promise<Commit[]> {
  const range = from ? `${from}..HEAD` : 'HEAD'
  const log = await $`git log --reverse --format=${`%H${FIELD}%an${FIELD}%s${FIELD}%b${RECORD}`} ${range}`.text()
  return log.split(RECORD).map(record => record.trim()).filter(Boolean).map((record) => {
    const [hash, author, subject, body] = record.split(FIELD)
    return { hash: hash!, author: author!, subject: subject!, body: (body ?? '').trim() }
  })
}

/** The commits the notes list: those that change the app, without the ones reverted in the same
 * range nor their reverts. */
export function changesOf(commits: Commit[]): Change[] {
  const reverted = new Set<string>()
  const hashes = commits.map(c => c.hash)
  for (const commit of commits) {
    const target = REVERTS.exec(commit.body)?.[1]
    const match = target && hashes.find(hash => hash.startsWith(target))
    if (match) {
      reverted.add(match).add(commit.hash)
    }
  }
  const changes: Change[] = []
  for (const commit of commits) {
    if (reverted.has(commit.hash)) {
      continue
    }
    const kind = commitType(commit.subject)
    const gitRevert = /^Revert "/.test(commit.subject)
    if (!kind && !gitRevert) {
      continue
    }
    const breaking = (kind?.breaking ?? false) || BREAKING.test(commit.body)
    const type = kind?.type ?? 'revert'
    if (!RELEASED_BY.includes(type) && type !== 'revert' && !breaking) {
      continue
    }
    const header = /^\w+(?:\(([^)]+)\))?!?: (.*)$/.exec(commit.subject)
    changes.push({
      ...commit,
      type,
      scope: header?.[1] ?? null,
      description: header?.[2] ?? commit.subject,
      breaking,
      authors: [],
    })
  }
  return changes
}

/** Whether these changes are worth a release: reverts alone are not. */
export function releases(changes: Change[]): boolean {
  return changes.some(change => change.type !== 'revert' || change.breaking)
}

/** The GitHub login of each author, so the release names and links them; their git name when
 * GitHub does not know the commit (not pushed yet, or an email of no account). */
export async function withAuthors(changes: Change[], repo: string): Promise<Change[]> {
  const token = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN
  const headers: Record<string, string> = { Accept: 'application/vnd.github+json' }
  if (token) {
    headers.Authorization = `Bearer ${token}`
  }
  return Promise.all(changes.map(async (change) => {
    let author = change.author
    try {
      const response = await fetch(`https://api.github.com/repos/${repo}/commits/${change.hash}`, { headers })
      const login = response.ok ? (await response.json() as { author?: { login?: string } }).author?.login : undefined
      if (login) {
        author = `@${login}`
      }
    }
    catch {}
    const coAuthors = change.body.split('\n')
      .map(line => TRAILER.exec(line))
      .filter(match => match?.[1]?.toLowerCase() === 'co-authored-by')
      .map(match => match![2]!)
    return { ...change, authors: [author, ...coAuthors.filter(name => name !== change.author)] }
  }))
}

function escape(text: string) {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function capitalize(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1)
}

function scopeName(scope: string) {
  return SCOPES[scope] ?? capitalize(scope)
}

/** A commit body as Markdown: its hard-wrapped lines joined, lists kept and trailers left out. */
export function unwrap(body: string): string {
  const paragraphs = body.split(/\n\s*\n/)
  return paragraphs.map((paragraph) => {
    const lines: string[] = []
    for (const line of paragraph.split('\n').map(l => l.trim())) {
      if (!line || TRAILER.test(line)) {
        continue
      }
      if (lines.length === 0 || /^([-*+]|\d+[.)]) /.test(line)) {
        lines.push(line)
      }
      else {
        lines[lines.length - 1] += ` ${line}`
      }
    }
    return lines.join('\n')
  }).filter(Boolean).join('\n\n')
}

function entry(change: Change, repo: string) {
  const scope = change.scope ? `**${escape(scopeName(change.scope))}:** ` : ''
  const commit = `[\`${change.hash.slice(0, 7)}\`](https://github.com/${repo}/commit/${change.hash})`
  const authors = change.authors.length ? change.authors : [change.author]
  const lines = [`- ${scope}${escape(capitalize(change.description))} (${commit}, by ${authors.map(escape).join(', ')})`]
  const body = unwrap(change.body)
  if (body) {
    const indented = escape(body).split('\n').map(line => line ? `  ${line}` : '').join('\n')
    lines.push('  <details><summary>Details</summary>', '', indented, '', '  </details>')
  }
  return lines.join('\n')
}

function grouped(changes: Change[]) {
  const groups = [{ title: '⚠️ Breaking changes', changes: changes.filter(c => c.breaking) }]
  for (const section of SECTIONS) {
    groups.push({ title: section.title, changes: changes.filter(c => !c.breaking && section.types.includes(c.type)) })
  }
  return groups.filter(group => group.changes.length > 0)
}

/** The notes of the GitHub release: every change with its author and whole message. */
export function releaseNotes(changes: Change[], options: { repo: string, last: string, tag: string }): string {
  const { repo, last, tag } = options
  if (!last) {
    return 'The first release of Taurilanzi.'
  }
  const sections = grouped(changes).map(group =>
    `## ${group.title}\n\n${group.changes.map(change => entry(change, repo)).join('\n')}`,
  )
  if (sections.length === 0) {
    sections.push(`Nothing in the app changed since ${last}.`)
  }
  return `${sections.join('\n\n')}\n\n**Every change since ${last}:** https://github.com/${repo}/compare/${last}...${tag}`
}

/** The notes of latest.json, in plain text: the subject of each change. */
export function shortNotes(changes: Change[], last: string): string {
  if (!last) {
    return 'The first release of Taurilanzi.'
  }
  return grouped(changes).map(group =>
    `${group.title.replace(/^\W+/, '')}:\n${group.changes.map((change) => {
      const scope = change.scope ? `${scopeName(change.scope)}: ` : ''
      return `- ${scope}${capitalize(change.description)}`
    }).join('\n')}`,
  ).join('\n\n') || `Nothing in the app changed since ${last}.`
}
