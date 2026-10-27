import type { Color } from '~/types/engine'

export function toHex({ r, g, b }: Color): string {
  return `#${[r, g, b].map(v => v.toString(16).padStart(2, '0')).join('')}`
}

export function fromHex(hex: string): Color {
  const value = Number.parseInt(hex.replace('#', ''), 16)
  return { r: (value >> 16) & 0xFF, g: (value >> 8) & 0xFF, b: value & 0xFF }
}

/** Hue in degrees (0–360), saturation and value from 0 to 1. */
export interface Hsv { h: number, s: number, v: number }

export function hexToHsv(hex: string): Hsv {
  const { r, g, b } = fromHex(hex)
  const [rn, gn, bn] = [r / 255, g / 255, b / 255]
  const max = Math.max(rn, gn, bn)
  const delta = max - Math.min(rn, gn, bn)
  let h = 0
  if (delta) {
    if (max === rn) {
      h = ((gn - bn) / delta) % 6
    }
    else if (max === gn) {
      h = (bn - rn) / delta + 2
    }
    else {
      h = (rn - gn) / delta + 4
    }
  }
  return { h: (h * 60 + 360) % 360, s: max ? delta / max : 0, v: max }
}

export function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6
    return Math.round((v - v * s * Math.max(0, Math.min(k, 4 - k, 1))) * 255)
  }
  return toHex({ r: f(5), g: f(3), b: f(1) })
}

/** `#abc`, `abc`, `#aabbcc` or `aabbcc` as `#aabbcc`, or null if it is not a color. */
export function normalizeHex(text: string): string | null {
  const value = text.trim().replace(/^#/, '').toLowerCase()
  if (/^[0-9a-f]{3}$/.test(value)) {
    return `#${value.split('').map(c => c + c).join('')}`
  }
  return /^[0-9a-f]{6}$/.test(value) ? `#${value}` : null
}
