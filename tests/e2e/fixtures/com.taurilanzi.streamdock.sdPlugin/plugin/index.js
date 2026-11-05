// The main service of the Stream Dock test plugin, as Mirabox's SDK starts one: the host's
// arguments in pairs, a registration, then events about each key. It needs nothing installed:
// the WebSocket of Node itself.
const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, value, i, all) => (i % 2 === 0 ? [...pairs, [value.replace(/^-/, ''), all[i + 1]]] : pairs), []),
)
const info = JSON.parse(args.info)
const counts = new Map()
const socket = new WebSocket(`ws://127.0.0.1:${args.port}`)
const send = message => socket.send(JSON.stringify(message))

function draw(context, count) {
  send({ event: 'setTitle', context, payload: { title: String(count), target: 0 } })
}

socket.onopen = () => {
  send({ event: args.registerEvent, uuid: args.pluginUUID })
  send({ event: 'logMessage', payload: { message: `registered as ${info.plugin.uuid} on ${info.application.platform}` } })
  send({ event: 'getGlobalSettings', context: args.pluginUUID })
}
socket.onclose = () => process.exit(0)
socket.onmessage = (event) => {
  const message = JSON.parse(event.data)
  const { context, payload } = message
  switch (message.event) {
    case 'willAppear':
      counts.set(context, payload.settings.count ?? 0)
      draw(context, counts.get(context))
      break
    case 'keyUp': {
      const count = (counts.get(context) ?? 0) + (payload.settings.step ?? 1)
      counts.set(context, count)
      send({ event: 'setSettings', context, payload: { ...payload.settings, count } })
      draw(context, count)
      break
    }
    case 'sendToPlugin':
      if (payload.ping !== undefined) {
        send({ event: 'sendToPropertyInspector', action: message.action, context, payload: { pong: payload.ping } })
      }
      break
  }
}
