import { $, browser, expect } from '@wdio/globals'
import {
  KEY_COUNT,
  button,
  chooseOption,
  dialog,
  clickOn,
  homePage,
  node,
  photoPng,
  pickFile,
  saved,
  setColor,
  shot,
  state,
  text,
} from '../helpers'

async function preferences() {
  return (await state()).preferences
}

async function tab(name: string) {
  await clickOn($(`//*[@role="tab"][normalize-space()="${name}"]`))
  await $(`//*[@role="tab"][normalize-space()="${name}"][@aria-selected="true"]`).waitForExist()
}

async function until(check: () => Promise<boolean>, timeoutMsg: string) {
  await browser.waitUntil(check, { timeoutMsg })
}

describe('the settings', () => {
  it('shows the account in the sidebar, which leads to the cloud', async () => {
    const account = $('[data-sidebar-account]')
    await expect(account).toBeDisplayed()
    await expect(account).toHaveText(expect.stringContaining('Cuenta'))
    await expect(account).toHaveText(expect.stringContaining('Conectar'))
  })

  it('opens from the sidebar with every section', async () => {
    await $('aside a[href="/settings"]').click()
    await expect($('h1=Ajustes')).toBeDisplayed()
    await expect($('[aria-label="Idioma"]')).toBeExisting()
    await tab('Dock')
    await expect($('[aria-label="Brillo del dock"]')).toBeExisting()
    await shot('settings')
    await tab('Bloqueo y reposo')
    await expect($('[aria-label="Imágenes del salvapantallas"] button[aria-label="Atardecer retro"]')).toHaveAttribute('aria-pressed', 'true')
    await shot('settings-screensaver')
  })

  it('saves the dock and what happens while locked or idle', async () => {
    await tab('Dock')
    await chooseOption('Suspensión del dock', '10 min')
    await tab('Bloqueo y reposo')
    await chooseOption('Salvapantallas tras un rato sin tocar el dock', '5 min')
    await clickOn(button('Apagar pantalla'))
    const aurora = $('button[aria-label="Aurora"]')
    await aurora.scrollIntoView({ block: 'center' })
    await clickOn(aurora)

    await until(async () => {
      const { screensaver } = await preferences()
      return screensaver?.idle_minutes === 5
        && screensaver?.on_lock === 'off'
        && screensaver?.image?.type === 'scene'
        && screensaver.image.scene === 'aurora'
    }, 'the preferences were not saved')
    await until(async () => (await state()).settings.sleep_minutes === 10, 'the sleep time was not saved')
    await expect(aurora).toHaveAttribute('aria-pressed', 'true')
    await expect(button('Probar en el dock')).toBeDisabled()
  })

  it('frames an image of your own for the screensaver and edits it again', async () => {
    const editor = dialog('Editar imagen')
    const apply = button('Aplicar', editor)
    await pickFile($('input[aria-label="Imagen del salvapantallas"]'), photoPng(800, 400), 'mine.png', 'image/png')
    await expect(editor).toBeDisplayed()
    await expect(apply).toBeEnabled()
    await shot('settings-screensaver-editor')
    await button('Girar a la derecha', editor).click()
    await apply.click()
    await expect(editor).not.toBeDisplayed()

    await until(async () => {
      const image = (await preferences()).screensaver?.image
      return image?.type === 'custom' && image.crop?.rotation === 90 && image.source !== image.asset
    }, 'the framed image was not chosen')
    await expect($('button[aria-label="Tu imagen"]')).toHaveAttribute('aria-pressed', 'true')

    await button('Editar imagen').click()
    // Closing unmounts the dialog, so the reopened one is found again.
    const reopened = dialog('Editar imagen')
    await expect(reopened).toBeDisplayed()
    await expect(button('Aplicar', reopened)).toBeEnabled()
    await button('Cancelar', reopened).click()
    await expect(reopened).not.toBeDisplayed()
  })

  it('creates keys in the style chosen for new keys', async () => {
    await tab('Teclas nuevas')
    await setColor('Fondo de las teclas nuevas', '#1d4ed8')
    await until(async () => (await preferences()).new_keys?.background?.b === 0xD8, 'the style was not saved')

    await clickOn($('[data-profile][aria-current="true"]'))
    const home = homePage(await state())
    const index = [...Array(KEY_COUNT).keys()].reverse().find(i => !home.keys[i])!
    await node(home.name).$(`button[aria-label="Tecla ${index + 1}"]`).click()
    await expect(text('Esta tecla está vacía.')).toBeDisplayed()
    await button('Configurar tecla').click()
    await saved()
    const key = homePage(await state()).keys[index]!
    expect(key.appearance.background).toEqual({ type: 'solid', color: { r: 0x1D, g: 0x4E, b: 0xD8 } })
  })
})
