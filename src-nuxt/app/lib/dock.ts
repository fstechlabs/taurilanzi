import type { Control, DeviceModel } from '~/types/engine'

/** A key at least this much wider than tall is a wide key. */
const WIDE = 1.6

/** The size a key on `index` is drawn at. */
export function keySize(model: DeviceModel | null, index: number) {
  const control = model?.controls.find(c => c.index === index)
  return control ? control.kind.image : { width: 196, height: 196 }
}

/** Width over height of the key on `index`. */
export function keyAspect(model: DeviceModel | null, index: number) {
  const size = keySize(model, index)
  return size.width / size.height
}

export function isWide(model: DeviceModel | null, index: number) {
  return keyAspect(model, index) >= WIDE
}

/** Rounded corners that look the same on a key of any shape: 9 % of its height. */
export function keyRadius(aspect: number) {
  return `${9 / aspect}% / 9%`
}

/** Where a control sits on the face of its dock, as percentages of the panel. */
export function placement(model: DeviceModel, control: Control) {
  const { width, height } = model.panel
  const b = control.bounds
  return {
    left: `${b.x / width * 100}%`,
    top: `${b.y / height * 100}%`,
    width: `${b.width / width * 100}%`,
    height: `${b.height / height * 100}%`,
  }
}

/** How plugins name the key on `index`: its column and row, `2_1`. */
export function keyName(model: DeviceModel | null, index: number) {
  const cell = model?.controls.find(c => c.index === index)?.cell
  return cell ? `${cell.column}_${cell.row}` : `${index % 5}_${Math.floor(index / 5)}`
}
