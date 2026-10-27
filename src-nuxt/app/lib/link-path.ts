export interface Point { x: number, y: number }

/**
 * Straight stretches between the points with rounded corners: the line goes exactly where the
 * user put the points, without the loops a smooth curve makes around sharp turns.
 */
export function roundedPath(points: Point[], radius: number): string {
  if (points.length < 2) {
    return ''
  }
  let d = `M ${points[0]!.x} ${points[0]!.y}`
  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1]!
    const p = points[i]!
    const next = points[i + 1]!
    const r = Math.min(radius, distance(prev, p) / 2, distance(p, next) / 2)
    const a = towards(p, prev, r)
    const b = towards(p, next, r)
    d += ` L ${a.x} ${a.y} Q ${p.x} ${p.y} ${b.x} ${b.y}`
  }
  const last = points.at(-1)!
  return `${d} L ${last.x} ${last.y}`
}

/** Index of the stretch of the line nearest to `p`. */
export function nearestStretch(points: Point[], p: Point): number {
  let best = 0
  let bestDistance = Infinity
  for (let i = 0; i < points.length - 1; i++) {
    const a = points[i]!
    const b = points[i + 1]!
    const dx = b.x - a.x
    const dy = b.y - a.y
    const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1)))
    const d = Math.hypot(a.x + t * dx - p.x, a.y + t * dy - p.y)
    if (d < bestDistance) {
      bestDistance = d
      best = i
    }
  }
  return best
}

function distance(a: Point, b: Point) {
  return Math.hypot(b.x - a.x, b.y - a.y)
}

function towards(from: Point, to: Point, length: number): Point {
  const l = distance(from, to) || 1
  return { x: from.x + (to.x - from.x) * length / l, y: from.y + (to.y - from.y) * length / l }
}
