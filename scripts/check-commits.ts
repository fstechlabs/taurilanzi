// Checks that commit messages follow Conventional Commits (https://www.conventionalcommits.org):
// `type(scope)!: description`, with the scope and the `!` optional. Whether main is released
// depends on these types (release-plan.ts).
// Usage: bun scripts/check-commits.ts --file <message file>   (the commit-msg hook)
//        bun scripts/check-commits.ts <from>..<to>            (every commit in a range)
import { $ } from 'bun'

const TYPES = ['feat', 'fix', 'perf', 'refactor', 'build', 'docs', 'test', 'ci', 'style', 'chore', 'revert']
const CONVENTIONAL = new RegExp(`^(${TYPES.join('|')})(\\([a-z0-9][a-z0-9._/-]*\\))?(!)?: \\S`)
// Written by git itself, not by people.
const GENERATED = /^(Merge |Revert "|fixup! |squash! |amend! )/

/** The type of a commit and whether it breaks something, or null for any other message. */
export function commitType(subject: string): { type: string, breaking: boolean } | null {
  const match = CONVENTIONAL.exec(subject)
  return match ? { type: match[1]!, breaking: match[3] === '!' } : null
}

export function isConventional(subject: string): boolean {
  return CONVENTIONAL.test(subject) || GENERATED.test(subject)
}

function explain(subjects: string[]) {
  console.error(`Not a Conventional Commit:\n${subjects.map(s => `  ${s}`).join('\n')}\n`)
  console.error(`Write it as \`type(scope): description\`, for example \`fix(driver): resend keys the dock dropped\`.`)
  console.error(`Types: ${TYPES.join(', ')}. Add \`!\` after the type or scope for a breaking change.`)
}

if (import.meta.main) {
  const args = process.argv.slice(2)
  let subjects: string[]
  if (args[0] === '--file') {
    const message = await Bun.file(args[1]!).text()
    subjects = [message.split('\n').find(line => line.trim() !== '' && !line.startsWith('#')) ?? '']
  }
  else {
    subjects = (await $`git log --format=%s ${args[0]!}`.text()).split('\n').filter(Boolean)
  }
  const wrong = subjects.filter(s => !isConventional(s))
  if (wrong.length > 0) {
    explain(wrong)
    process.exit(1)
  }
}
