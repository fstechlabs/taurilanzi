import type { KeyCombo } from '~/types/engine'
import { t } from '~/lib/i18n'
import type { Os } from '~/lib/os'
import { os } from '~/lib/os'

/** Keys that only change others; a shortcut needs one more key besides them. */
const MODIFIERS = new Set(['MetaLeft', 'MetaRight', 'ShiftLeft', 'ShiftRight', 'AltLeft', 'AltRight', 'ControlLeft', 'ControlRight', 'CapsLock', 'Fn'])

/** Glyphs as Mac keyboards print them. */
const MAC_NAMED: Record<string, string> = {
  Enter: '↩',
  NumpadEnter: '⌤',
  Tab: '⇥',
  Backspace: '⌫',
  Delete: '⌦',
  IntlBackslash: '§',
  NumLock: 'Clear',
}

const NAMED: Record<string, string> = {
  Escape: 'Esc',
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
  Insert: 'Insert',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: '\'',
  Backquote: '`',
  Comma: ',',
  Period: '.',
  Slash: '/',
  IntlBackslash: '<',
  NumpadDecimal: 'Num .',
  NumpadMultiply: 'Num *',
  NumpadAdd: 'Num +',
  NumpadSubtract: 'Num -',
  NumpadDivide: 'Num /',
  NumpadEqual: 'Num =',
}

/** Keys named with words, in the language shown. */
const WORDS: Record<string, string> = {
  Space: 'space',
  Home: 'home',
  End: 'end',
  PageUp: 'pageUp',
  PageDown: 'pageDown',
  Enter: 'enter',
  NumpadEnter: 'numpadEnter',
  Tab: 'tab',
  Backspace: 'backspace',
  Delete: 'delete',
  NumLock: 'numLock',
}

export type Modifier = 'control' | 'option' | 'shift' | 'command'

/** In the order each system writes them. */
function modifierOrder(system: Os): Modifier[] {
  return system === 'macos' ? ['control', 'option', 'shift', 'command'] : ['command', 'control', 'option', 'shift']
}

export const MODIFIER_ORDER = modifierOrder(os)

const MAC_SYMBOLS: Record<Modifier, string> = { control: '⌃', option: '⌥', shift: '⇧', command: '⌘' }

/** How the key is printed on the keyboard: symbols on a Mac, short words elsewhere. */
export function modifierSymbol(modifier: Modifier, system: Os = os) {
  if (system === 'macos') {
    return MAC_SYMBOLS[modifier]
  }
  return modifierName(modifier, system)
}

/** The key's name, in the language shown. */
export function modifierName(modifier: Modifier, system: Os = os) {
  if (system === 'macos' || modifier === 'control' || modifier === 'shift') {
    return t(`actions.hotkey.${modifier}`)
  }
  if (modifier === 'option') {
    return 'Alt'
  }
  return system === 'windows' ? 'Win' : 'Super'
}

/** A physical key as printed on a US keyboard, which is how the shortcut is stored. */
export function keyLabel(code: string, system: Os = os) {
  if (system === 'macos' && MAC_NAMED[code]) {
    return MAC_NAMED[code]
  }
  if (WORDS[code]) {
    return t(`actions.keys.${WORDS[code]}`)
  }
  if (NAMED[code]) {
    return NAMED[code]
  }
  if (code.startsWith('Key')) {
    return code.slice(3)
  }
  if (code.startsWith('Digit')) {
    return code.slice(5)
  }
  if (code.startsWith('Numpad')) {
    return `Num ${code.slice(6)}`
  }
  return code
}

/** `⌃⇧A` on a Mac, `Ctrl+Shift+A` elsewhere. */
export function comboLabel(keys: KeyCombo, system: Os = os) {
  if (!keys.key) {
    return ''
  }
  const parts = modifierOrder(system).filter(m => keys[m]).map(m => modifierSymbol(m, system))
  parts.push(keyLabel(keys.key, system))
  return parts.join(system === 'macos' ? '' : '+')
}

/** Undo and redo as each system writes them. */
export const UNDO_SHORTCUT = comboLabel({ key: 'KeyZ', command: os === 'macos', control: os !== 'macos', shift: false, option: false })
export const REDO_SHORTCUT = os === 'windows'
  ? comboLabel({ key: 'KeyY', control: true, command: false, shift: false, option: false })
  : comboLabel({ key: 'KeyZ', command: os === 'macos', control: os !== 'macos', shift: true, option: false })

export function isModifier(code: string) {
  return MODIFIERS.has(code)
}

/** Every key the shortcut can end in, for choosing it without pressing it. */
export const KEY_CODES: string[] = [
  ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').map(l => `Key${l}`),
  ...'0123456789'.split('').map(d => `Digit${d}`),
  ...Array.from({ length: 20 }, (_, i) => `F${i + 1}`),
  'Enter', 'Tab', 'Space', 'Backspace', 'Delete', 'Escape',
  'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown',
  'Minus', 'Equal', 'BracketLeft', 'BracketRight', 'Backslash', 'Semicolon', 'Quote', 'Backquote', 'Comma', 'Period', 'Slash', 'IntlBackslash',
  ...'0123456789'.split('').map(d => `Numpad${d}`),
  'NumpadDecimal', 'NumpadMultiply', 'NumpadAdd', 'NumpadSubtract', 'NumpadDivide', 'NumpadEqual', 'NumpadEnter', 'NumLock',
]
