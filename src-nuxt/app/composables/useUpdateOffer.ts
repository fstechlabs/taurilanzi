import { commands, events, type UpdateOffer } from '~/ipc/bindings'

const offer = ref<UpdateOffer | null>(null)
let started = false

/** The downloaded update the window offers to install, kept up to date by the engine. */
export function useUpdateOffer() {
  if (!started && import.meta.client) {
    started = true
    events.updateOffered.listen(({ payload }) => {
      offer.value = payload
    })
    commands.updateOffer().then((value) => {
      offer.value = value
    })
  }

  return {
    offer: readonly(offer),
    install: () => commands.installUpdate(),
    remindLater: () => commands.remindUpdateLater(),
  }
}
