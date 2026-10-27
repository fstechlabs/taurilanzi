import type { Crop } from '~/types/engine'

export const ZOOM_MAX = 8
/** The edited image is saved at twice the key size, so it stays sharp if the key is reused larger. */
const OUTPUT_SCALE = 2

export interface Size { width: number, height: number }

/** A loaded source image, ready to be shown in the editor and drawn into the result. */
export interface SourceImage {
  url: string
  natural: Size
  /** SVG markup, kept so icons stay vector and keep `currentColor` for the key tint. */
  svg: string | null
}

export function identityCrop(): Crop {
  return { zoom: 1, x: 0, y: 0, rotation: 0, flip_x: false, flip_y: false }
}

/** Scale at zoom 1: the image covers the frame exactly. */
function baseScale(natural: Size, frame: Size) {
  return Math.max(frame.width / natural.width, frame.height / natural.height)
}

function rotated(rotation: number) {
  const r = rotation * Math.PI / 180
  return { c: Math.abs(Math.cos(r)), s: Math.abs(Math.sin(r)) }
}

/** Zoom at which the rotated image covers the whole frame. */
export function fillZoom(natural: Size, frame: Size, rotation: number) {
  const { c, s } = rotated(rotation)
  const scale = Math.max(
    (frame.width * c + frame.height * s) / natural.width,
    (frame.width * s + frame.height * c) / natural.height,
  )
  return scale / baseScale(natural, frame)
}

/** Zoom at which the whole rotated image fits inside the frame. */
export function fitZoom(natural: Size, frame: Size, rotation: number) {
  const { c, s } = rotated(rotation)
  const scale = Math.min(
    frame.width / (natural.width * c + natural.height * s),
    frame.height / (natural.width * s + natural.height * c),
  )
  return scale / baseScale(natural, frame)
}

export function minZoom(natural: Size, frame: Size) {
  return Math.min(1, fitZoom(natural, frame, 0)) / 2
}

export function clampZoom(zoom: number, natural: Size, frame: Size) {
  return Math.min(ZOOM_MAX, Math.max(minZoom(natural, frame), zoom))
}

/** Where and how big the image is drawn inside a frame of the given size. */
export function placement(crop: Crop, natural: Size, frame: Size) {
  const scale = baseScale(natural, frame) * crop.zoom
  return {
    width: natural.width * scale,
    height: natural.height * scale,
    centerX: frame.width / 2 + crop.x * frame.width,
    centerY: frame.height / 2 + crop.y * frame.height,
    flipX: crop.flip_x ? -1 : 1,
    flipY: crop.flip_y ? -1 : 1,
  }
}

/**
 * Keeps the image point under `point` (relative to the frame center, in pixels) still while
 * zooming, the way maps and photo editors do.
 */
export function zoomAround(crop: Crop, zoom: number, point: { x: number, y: number }, frame: Size): Crop {
  const k = zoom / crop.zoom
  const ox = crop.x * frame.width
  const oy = crop.y * frame.height
  return {
    ...crop,
    zoom,
    x: (point.x + k * (ox - point.x)) / frame.width,
    y: (point.y + k * (oy - point.y)) / frame.height,
  }
}

export function outputSize(aspect: number): Size {
  const height = 196 * OUTPUT_SCALE
  return { width: Math.round(height * aspect), height }
}

export async function loadSource(blob: Blob): Promise<SourceImage> {
  if (blob.type === 'image/svg+xml') {
    const svg = await blob.text()
    return { url: URL.createObjectURL(blob), natural: svgSize(svg), svg }
  }
  const url = URL.createObjectURL(blob)
  const image = await loadImage(url)
  return { url, natural: { width: image.naturalWidth, height: image.naturalHeight }, svg: null }
}

/** Draws the framed image at its final size: SVG stays SVG, everything else becomes PNG. */
export async function bake(source: SourceImage, crop: Crop, size: Size): Promise<Blob> {
  const p = placement(crop, source.natural, size)
  if (source.svg !== null) {
    return new Blob([bakeSvg(source.svg, source.natural, p, crop.rotation, size)], { type: 'image/svg+xml' })
  }
  const canvas = document.createElement('canvas')
  canvas.width = size.width
  canvas.height = size.height
  const ctx = canvas.getContext('2d')!
  ctx.imageSmoothingQuality = 'high'
  ctx.translate(p.centerX, p.centerY)
  ctx.rotate(crop.rotation * Math.PI / 180)
  ctx.scale(p.flipX, p.flipY)
  ctx.drawImage(await loadImage(source.url), -p.width / 2, -p.height / 2, p.width, p.height)
  return await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('could not encode the image')), 'image/png'))
}

function bakeSvg(markup: string, natural: Size, p: ReturnType<typeof placement>, rotation: number, size: Size) {
  const doc = new DOMParser().parseFromString(markup, 'image/svg+xml')
  const inner = doc.documentElement
  inner.setAttribute('width', String(natural.width))
  inner.setAttribute('height', String(natural.height))
  const sx = p.flipX * p.width / natural.width
  const sy = p.flipY * p.height / natural.height
  const transform = `translate(${p.centerX} ${p.centerY}) rotate(${rotation}) scale(${sx} ${sy}) translate(${-natural.width / 2} ${-natural.height / 2})`
  const body = new XMLSerializer().serializeToString(inner)
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size.width}" height="${size.height}" viewBox="0 0 ${size.width} ${size.height}"><g transform="${transform}">${body}</g></svg>`
}

function svgSize(markup: string): Size {
  const root = new DOMParser().parseFromString(markup, 'image/svg+xml').documentElement
  const box = root.getAttribute('viewBox')?.trim().split(/[\s,]+/).map(Number)
  if (box?.length === 4 && box[2]! > 0 && box[3]! > 0) {
    return { width: box[2]!, height: box[3]! }
  }
  const width = Number.parseFloat(root.getAttribute('width') ?? '')
  const height = Number.parseFloat(root.getAttribute('height') ?? '')
  return width > 0 && height > 0 ? { width, height } : { width: 100, height: 100 }
}

function loadImage(url: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = () => reject(new Error('could not read the image'))
    image.src = url
  })
}
