import { entryFor } from '~/lib/actions/catalog'
import { switchLabels, tileAction } from '~/lib/actions/hass'
import { httpSummary } from '~/lib/actions/http'
import { comboLabel } from '~/lib/actions/keys'
import { findAction } from '~/lib/actions/plugins'
import { NO_SERVER } from '~/lib/integrations'
import type { Action, HassCommand } from '~/types/engine'

function camel(text: string) {
  return text.replace(/_(\w)/g, (_, letter: string) => letter.toUpperCase())
}

/** The last part of a path, without the `.app` of a Mac app. */
function fileName(path: string) {
  return (path.split(/[\\/]/).filter(Boolean).pop() ?? path).replace(/\.app$/, '')
}

/** What an action does, in a sentence for the list of a key's actions. */
export function useActionSummary() {
  const { t, locale } = useI18n()
  const { snapshot } = useEngine()
  const { pages } = useWorkspace()
  const integrations = useIntegrations()
  const { catalog } = usePlugins()
  const servers = new Map<string, ReturnType<typeof useHass>>()
  // Servers are met while drawing the list, outside setup: their watchers live in a scope of
  // their own, stopped with the component.
  const scope = effectScope()
  onScopeDispose(() => scope.stop())

  function hassOf(server: string) {
    if (!servers.has(server)) {
      servers.set(server, scope.run(() => useHass(server))!)
    }
    return servers.get(server)!
  }

  function hassDoes(command: HassCommand): string {
    switch (command.kind) {
      case 'switch': {
        const labels = switchLabels(command.entity)
        return !labels ? t('actions.hass.onlyRuns') : command.mode === 'toggle' ? t('actions.hass.toggle') : labels[command.mode]
      }
      case 'level': {
        const change = command.change
        const amount = change.type === 'set' ? `= ${change.to}` : `${change.by > 0 ? '+' : ''}${change.by}`
        return `${t(`actions.hass.levels.${camel(command.level)}`)} ${amount}`
      }
      case 'choose':
        return `${t(`actions.hass.options.${camel(command.option)}`)}: ${command.choice.type === 'set' ? command.choice.option : t(`actions.hass.${command.choice.type}`)}`
      case 'media':
        return t(`actions.hass.media.${camel(command.control)}`)
      case 'alarm':
        return t(`actions.hass.alarm.${camel(command.mode)}`)
      case 'color':
        return t('actions.hass.color')
      case 'show':
        return t('actions.catalog.entries.homeAssistant.show.name')
      case 'tile':
        return (command.press !== 'nothing' && tileAction(command.entity)) || t('actions.hass.tile.nothing')
      default:
        return ''
    }
  }

  function hassSummary(server: string, command: HassCommand): string {
    const hass = hassOf(server)
    const name = (id: string) => hass.entity(id)?.name ?? id
    switch (command.kind) {
      case 'call': {
        if (!command.action) {
          return t('actions.hass.call.chooseAction')
        }
        const target = [...(command.target?.entities ?? []).map(name), ...(command.target?.areas ?? []).map(id => hass.areaName(id) ?? id)]
        return [hass.serviceName(command.action), target.join(', ')].filter(Boolean).join(' · ')
      }
      case 'assist':
        return command.text ? t('actions.summary.assist', { text: command.text }) : t('actions.hass.assistText')
      case 'event':
        return command.event ? t('actions.summary.event', { event: command.event }) : t('actions.hass.eventType')
      default:
        return command.entity ? `${name(command.entity)} · ${hassDoes(command)}` : t('actions.hass.chooseEntity')
    }
  }

  function serverName(id: string | undefined) {
    return integrations.server(id)?.name ?? t('integrations.errors.choose')
  }

  function summary(action: Action): string {
    const entry = entryFor(action)
    switch (action.type) {
      case 'open_url':
        return action.url.trim() ? t('actions.summary.openUrl', { url: action.url.trim() }) : t('actions.summary.noAddress')
      case 'open':
        return action.path ? t('actions.summary.open', { name: fileName(action.path) }) : t('editor.validation.open')
      case 'close_app':
        return action.app ? t('actions.summary.closeApp', { name: fileName(action.app) }) : t('editor.validation.closeApp')
      case 'type_text':
        if (!action.text) {
          return t('editor.validation.typeText')
        }
        return t(action.enter ? 'actions.summary.typeTextEnter' : 'actions.summary.typeText', { text: action.text.replace(/\s+/g, ' ') })
      case 'hotkey':
        return action.keys.key ? t('actions.summary.hotkey', { keys: comboLabel(action.keys) }) : t('editor.validation.hotkey')
      case 'open_page': {
        const page = pages.value.find(p => p.id === action.page)
        return page ? t('actions.summary.openPage', { name: page.name }) : t('editor.validation.openPage')
      }
      case 'switch_profile': {
        const profile = snapshot.value?.profiles.find(p => p.id === action.profile)
        return profile ? t('actions.summary.switchProfile', { name: profile.name }) : t('editor.validation.switchProfile')
      }
      case 'delay':
        return t('actions.summary.delay', { seconds: (action.ms / 1000).toLocaleString(locale.value, { maximumFractionDigits: 1 }) })
      case 'random':
        return action.actions.length ? t('actions.summary.random', action.actions.length) : t('editor.validation.random')
      case 'play_sound':
        return action.sound ? t('actions.summary.playSound', { name: action.name }) : t('editor.validation.playSound')
      case 'obs':
        return `${entry?.name ?? ''} · ${serverName(action.server)}`
      case 'http': {
        const { title } = httpSummary(action.request)
        return title
      }
      case 'home_assistant':
        return hassSummary(action.server ?? NO_SERVER, action.command)
      case 'plugin': {
        const found = findAction(catalog.value, action.action)
        return found ? `${found.plugin.name} · ${found.action.name}` : t('actions.details.pluginMissing')
      }
      default:
        return entry?.description ?? ''
    }
  }

  return { summary }
}
