// The version of a release is when it was built, in UTC: YEAR.MONTHDAY.HOURMINUTE, such as
// 2026.1220.2008 for 20:08 on 20 December 2026. Semantic versions forbid leading zeros, so
// 5 January at 09:30 is 2027.105.930: the last two digits are always the day and the minutes.
// UTC, because local time repeats an hour every autumn and a version must only ever grow.
// Usage: bun scripts/release-version.ts   prints the version for now

export function releaseVersion(date = new Date()): string {
  const day = (date.getUTCMonth() + 1) * 100 + date.getUTCDate()
  const time = date.getUTCHours() * 100 + date.getUTCMinutes()
  return `${date.getUTCFullYear()}.${day}.${time}`
}

/** When a version was built, as people write it: `2027-01-05 09:30 UTC`. */
export function releaseDate(version: string): string {
  const [year, day, time] = version.split('.').map(Number) as [number, number, number]
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${year}-${pad(Math.floor(day / 100))}-${pad(day % 100)} ${pad(Math.floor(time / 100))}:${pad(time % 100)} UTC`
}

/** Whether `a` comes after `b`, comparing their three numbers in order. */
export function isNewer(a: string, b: string): boolean {
  const x = a.split('.').map(Number)
  const y = b.split('.').map(Number)
  for (let i = 0; i < 3; i++) {
    if (x[i] !== y[i]) {
      return x[i]! > y[i]!
    }
  }
  return false
}

if (import.meta.main) {
  console.log(releaseVersion())
}
