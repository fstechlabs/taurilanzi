/** The golden angle spreads consecutive hues as far apart as possible around the wheel. */
const GOLDEN_ANGLE = 137.508
/** Two lines on the canvas never get hues closer than this. */
const MIN_HUE_GAP = 40

function hash(text: string) {
  let h = 2166136261
  for (let i = 0; i < text.length; i++) {
    h = Math.imul(h ^ text.charCodeAt(i), 16777619)
  }
  return h >>> 0
}

function gap(a: number, b: number) {
  const d = Math.abs(a - b) % 360
  return Math.min(d, 360 - d)
}

/**
 * A colour for each line, clearly apart from the others. Each line starts from a hue taken from
 * its id and only moves away from it if another line already uses something close, so colours
 * stay put as lines come and go.
 */
export function linkColors(ids: string[]): Map<string, string> {
  const taken: number[] = []
  const colors = new Map<string, string>()
  for (const id of [...ids].sort()) {
    const preferred = (hash(id) * GOLDEN_ANGLE) % 360
    const spacing = (hue: number) => Math.min(360, ...taken.map(t => gap(hue, t)))
    // The free hue nearest to the preferred one; if none is free, the one furthest from the rest.
    let hue = preferred
    let best = { free: false, near: Infinity, spacing: -1 }
    for (let degree = 0; degree < 360; degree++) {
      const free = spacing(degree) >= MIN_HUE_GAP
      const near = gap(degree, preferred)
      const better = free
        ? !best.free || near < best.near
        : !best.free && spacing(degree) > best.spacing
      if (better) {
        best = { free, near, spacing: spacing(degree) }
        hue = degree
      }
    }
    taken.push(hue)
    colors.set(id, `hsl(${Math.round(hue)} 85% 62%)`)
  }
  return colors
}
