<script setup lang="ts">
import { KeyRound, Trash2 } from '@lucide/vue'
import { commands } from '~/ipc/bindings'

/** The code of an alarm panel or a lock: kept with the secrets, never shown again. */
const props = defineProps<{ entity: string }>()

const { t } = useI18n()
const hass = useHass()
const saved = ref(false)
const code = ref('')
const busy = ref(false)
const failed = ref(false)

/** Home Assistant says whether the entity asks for a code at all. */
const asks = computed(() => {
  const format = hass.entity(props.entity)?.attributes.code_format
  return format !== undefined && format !== null
})

async function refresh() {
  try {
    saved.value = await call(commands.hassCodeSaved(hass.id.value, props.entity))
  }
  catch {
    saved.value = false
  }
}

watch(() => props.entity, refresh, { immediate: true })

async function save(value: string | null) {
  busy.value = true
  failed.value = false
  try {
    await call(commands.hassSetCode(hass.id.value, props.entity, value))
    code.value = ''
    await refresh()
  }
  catch {
    failed.value = true
  }
  finally {
    busy.value = false
  }
}
</script>

<template>
  <div v-if="asks || saved" class="flex flex-col gap-1.5 rounded-md border border-dashed p-2">
    <div class="flex items-center gap-1.5">
      <KeyRound class="size-3.5 shrink-0 text-muted-foreground" />
      <Input
        v-model="code"
        type="password"
        autocomplete="off"
        class="h-8 flex-1"
        :placeholder="saved ? '••••' : t('actions.hass.code')"
        :aria-label="t('actions.hass.code')"
        @keydown.enter="code && save(code)"
      />
      <Button variant="outline" size="sm" :disabled="!code || busy" @click="save(code)">
        {{ t('actions.hass.saveCode') }}
      </Button>
      <Button v-if="saved" variant="ghost" size="icon-sm" :disabled="busy" :aria-label="t('actions.hass.removeCode')" :title="t('actions.hass.removeCode')" @click="save(null)">
        <Trash2 />
      </Button>
    </div>
    <p class="text-[11px]" :class="failed ? 'text-destructive' : 'text-muted-foreground'">
      {{ failed ? t('actions.hass.codeFailed') : saved ? t('actions.hass.codeSaved') : t('actions.hass.codeHint') }}
    </p>
  </div>
</template>
