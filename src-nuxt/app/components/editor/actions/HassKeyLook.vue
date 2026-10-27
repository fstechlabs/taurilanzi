<script setup lang="ts">
import { RotateCcw, Search } from '@lucide/vue'
import type { PickedIcon } from '~/composables/useIconCatalog'
import { KEY_DRAFT } from '~/lib/actions/key-draft'
import { followedAction, followedState, sameCommand } from '~/lib/actions/hass'
import { preload, thumbnail } from '~/lib/iconify'
import type { HassCommand } from '~/types/engine'

/** How the key shows the entity of a command: as a card of the entity, or with an image and a
 * text of its own; its icon and its name, Home Assistant's unless changed here. */
const props = defineProps<{ command: HassCommand, server: string }>()

const { t } = useI18n()
const hass = useHass(() => props.server)
const { assetUrl } = useEngine()
const draft = inject(KEY_DRAFT, null)
const look = useHassKey(draft, () => props.server)
const iconsLoaded = ref(0)
const busy = ref(false)
const failed = ref(false)
const picking = ref(false)

const entity = computed(() => {
  const command = props.command
  if (command.kind === 'call') {
    return command.target?.entities?.[0] ?? ''
  }
  return 'entity' in command ? command.entity : ''
})
const info = computed(() => (entity.value ? hass.entity(entity.value) : undefined))
const entityName = computed(() => info.value?.name ?? entity.value)

const state = computed(() => followedState(props.command))
const followed = computed(() => (draft?.value ? followedAction(draft.value.actions, draft.value.second?.actions ?? []) : null))
const isFollowed = computed(() => followed.value?.type === 'home_assistant' && sameCommand(followed.value.command, props.command))
const follows = computed(() => Boolean(draft?.value?.second && isFollowed.value))
const hasGestures = computed(() => Boolean(draft?.value?.doublePress.length || draft?.value?.longPress.length))
/** Another action of the key already decides its two states. */
const followsOther = computed(() => Boolean(draft?.value?.second && !isFollowed.value))

const card = computed(() => look.card.value)
const showsCard = computed(() => Boolean(card.value))

/** The icon the key shows now, Home Assistant's for the state the entity is in when not changed. */
const haIcon = computed(() => (entity.value ? hass.entityIcon(entity.value) : null))
watch(haIcon, async (name) => {
  if (name) {
    await preload([name])
    iconsLoaded.value++
  }
}, { immediate: true })
const icon = computed(() => {
  void iconsLoaded.value
  if (card.value?.icon) {
    return { src: assetUrl(card.value.icon), own: true, name: null, invert: true }
  }
  const image = draft?.value?.image
  if (!card.value && image) {
    const own = image.icon !== hass.entityIcon(entity.value, '')
    return { src: assetUrl(image.asset), own, name: image.icon, invert: Boolean(image.icon) }
  }
  return { src: haIcon.value ? thumbnail(haIcon.value) : null, own: false, name: haIcon.value, invert: false }
})

const name = computed({
  get: () => (card.value ? card.value.label : draft?.value?.text?.content ?? ''),
  set: (value: string) => {
    if (card.value) {
      look.setCardLabel(value)
    }
    else {
      look.setText(value)
    }
  },
})

async function run(task: () => Promise<void> | void) {
  busy.value = true
  failed.value = false
  try {
    await task()
  }
  catch {
    failed.value = true
  }
  finally {
    busy.value = false
  }
}

function setCard(on: boolean) {
  run(() => (on ? look.showCard(entity.value) : look.hideCard()))
}

function pick({ icon: picked }: PickedIcon) {
  run(() => (card.value ? look.setCardIcon(picked) : look.setImage(entity.value, props.command, picked)))
}

function resetIcon() {
  run(() => (card.value ? look.setCardIcon(null) : look.setImage(entity.value, props.command, null)))
}

function resetName() {
  name.value = card.value ? '' : entityName.value
}

function setFollow(on: boolean) {
  run(() => (on ? look.follow(props.command) : look.unfollow()))
}

const onColor = computed({
  get: () => draft?.value?.second?.from ?? '#d97706',
  set: (hex: string) => {
    const second = draft?.value?.second
    if (second) {
      second.background = 'solid'
      second.from = hex
    }
  },
})
const offColor = computed({
  get: () => draft?.value?.from ?? '#000000',
  set: (hex: string) => {
    if (draft?.value) {
      draft.value.background = 'solid'
      draft.value.from = hex
    }
  },
})
</script>

<template>
  <div v-if="draft && entity" class="flex flex-col gap-4 rounded-md border p-3" data-hass-look>
    <p class="text-xs font-medium">
      {{ t('actions.hass.look.title') }}
    </p>

    <label class="flex items-center justify-between gap-2 text-sm">
      <span>
        {{ t('actions.hass.look.card') }}
        <span class="block text-[11px] text-muted-foreground">{{ t('actions.hass.look.cardHint') }}</span>
      </span>
      <Switch :model-value="showsCard" :disabled="busy" :aria-label="t('actions.hass.look.card')" @update:model-value="setCard" />
    </label>

    <div class="flex items-center gap-3">
      <div class="flex size-14 shrink-0 items-center justify-center rounded-lg bg-neutral-800 p-2.5" data-hass-icon>
        <img v-if="icon.src" :src="icon.src" alt="" class="size-full" :class="icon.invert ? 'invert' : ''">
      </div>
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <span class="text-xs text-muted-foreground">{{ t('actions.hass.look.icon') }}</span>
        <span class="truncate text-sm">{{ icon.own ? (icon.name ?? t('actions.hass.look.ownIcon')) : t(showsCard ? 'actions.hass.look.haIconLive' : 'actions.hass.look.haIcon') }}</span>
      </div>
      <Button variant="outline" size="sm" :disabled="busy || !info" @click="picking = true">
        <Search /> {{ t('actions.hass.look.changeIcon') }}
      </Button>
      <Button v-if="icon.own" variant="ghost" size="icon-sm" :disabled="busy || !info" :aria-label="t('actions.hass.look.resetIcon')" :title="t('actions.hass.look.resetIcon')" @click="resetIcon">
        <RotateCcw />
      </Button>
    </div>

    <div class="flex flex-col gap-1.5">
      <Label class="text-xs text-muted-foreground">{{ t(showsCard ? 'actions.hass.look.name' : 'actions.hass.look.text') }}</Label>
      <div class="flex items-center gap-1.5">
        <Input v-model="name" :placeholder="entityName" maxlength="40" class="flex-1" :aria-label="t(showsCard ? 'actions.hass.look.name' : 'actions.hass.look.text')" />
        <Button v-if="showsCard ? Boolean(name) : name !== entityName" variant="ghost" size="icon-sm" :aria-label="t('actions.hass.look.resetName')" :title="t('actions.hass.look.resetName')" @click="resetName">
          <RotateCcw />
        </Button>
      </div>
      <p class="text-[11px] text-muted-foreground">
        {{ t(showsCard ? 'actions.hass.look.nameHint' : 'actions.hass.look.textHint') }}
      </p>
    </div>

    <template v-if="state">
      <label class="flex items-center justify-between gap-2 text-sm">
        <span>
          {{ t('actions.hass.look.background') }}
          <span class="block text-[11px] text-muted-foreground">
            {{ hasGestures ? t('actions.hass.removeGestures') : followsOther ? t('actions.hass.look.followsOther') : t('actions.hass.giveStates', { state: state.while }) }}
          </span>
        </span>
        <Switch :model-value="follows" :disabled="busy || (!follows && (hasGestures || followsOther))" :aria-label="t('actions.hass.look.background')" @update:model-value="setFollow" />
      </label>
      <div v-if="follows" class="flex flex-wrap items-center gap-4 text-xs">
        <span class="flex items-center gap-2">
          <ColorPicker v-model="offColor" :label="t('actions.hass.look.offColor')" />
          {{ t('actions.hass.look.offColor') }}
        </span>
        <span class="flex items-center gap-2">
          <ColorPicker v-model="onColor" :label="t('actions.hass.look.onColor')" />
          {{ t('actions.hass.look.onColor') }}
        </span>
      </div>
    </template>

    <p v-if="failed" class="text-[11px] text-destructive">
      {{ t('actions.hass.look.failed') }}
    </p>
    <IconPicker v-model:open="picking" @pick="pick" />
  </div>
</template>
