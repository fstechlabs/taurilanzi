import { expect, test } from 'bun:test'
import { failure, reasonOf } from '../../src-nuxt/app/lib/errors'

test('an error says what failed and why', () => {
  const refused = new Error('Es el único perfil de su dock.')
  expect(failure('No se ha podido mover el perfil.', refused)).toBe('No se ha podido mover el perfil: Es el único perfil de su dock.')
  expect(failure('No se ha podido mover el perfil', 'sin conexión')).toBe('No se ha podido mover el perfil: sin conexión')
})

test('without a reason the error says what failed', () => {
  expect(failure('No se pudo guardar.', new Error(' '))).toBe('No se pudo guardar.')
  expect(reasonOf(42)).toBe('42')
})
