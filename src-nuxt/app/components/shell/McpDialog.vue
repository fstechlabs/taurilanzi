<script setup lang="ts">
import { Check, ChevronRight, Copy, Eye, EyeOff, KeyRound, LoaderCircle, RefreshCw, Sparkles } from '@lucide/vue'
import { ASSISTANT_NAMES, ASSISTANTS, clientName, snippets, timeAgo, TOKEN_ENV } from '~/lib/mcp'
import type { Assistant } from '~/lib/mcp'
import { os } from '~/lib/os'

const open = defineModel<boolean>('open', { required: true })

const { t, locale } = useI18n()
const { show } = useToasts()
const mcp = useMcp()

const MASK = '•'.repeat(16)
const COPIED_MS = 1500
const CLOCK_MS = 30_000

const status = computed(() => mcp.status.value)
const busy = ref(false)
const revealed = ref(false)
const tokenOpen = ref(false)
const assistant = ref<Assistant>('claude-code')
const confirming = ref(false)
const copied = ref<string | null>(null)
let copiedTimer: ReturnType<typeof setTimeout> | undefined
const now = ref(new Date())
let clock: ReturnType<typeof setInterval> | undefined

watch(open, (value) => {
  clearInterval(clock)
  if (value) {
    now.value = new Date()
    clock = setInterval(() => {
      now.value = new Date()
    }, CLOCK_MS)
  }
  else {
    revealed.value = false
    tokenOpen.value = false
  }
}, { immediate: true })
onBeforeUnmount(() => {
  clearInterval(clock)
  clearTimeout(copiedTimer)
})

const dot = computed(() => ({
  off: 'bg-neutral-500',
  starting: 'bg-amber-400',
  running: 'bg-emerald-500',
  error: 'bg-red-500',
})[mcp.state.value])

const statusText = computed(() => {
  const value = status.value
  switch (mcp.state.value) {
    case 'running':
      return t('mcp.status.running', { url: value?.url ?? '' })
    case 'error':
      return value?.error ?? ''
    case 'starting':
      return t('mcp.status.starting')
    default:
      return t('mcp.status.off')
  }
})

const lastUsed = computed(() => {
  const value = status.value
  if (!value?.last_used) {
    return null
  }
  const when = new Date(value.last_used)
  if (Number.isNaN(when.getTime())) {
    return null
  }
  const client = value.last_client ? clientName(value.last_client) ?? t('mcp.status.bridgeClient') : t('mcp.status.someClient')
  return t('mcp.status.lastUsed', { client, when: timeAgo(when, now.value, locale.value) })
})

/** Only while it is on is there a token, and so anything to paste in an assistant. */
const connection = computed(() => {
  const value = status.value
  return value?.enabled && value.token ? { url: value.url, token: value.token, bridge: value.bridge } : null
})

const blocks = computed(() => {
  const live = connection.value
  if (!live) {
    return []
  }
  const real = snippets(assistant.value, live, os)
  const shown = snippets(assistant.value, { ...live, token: revealed.value ? live.token : MASK }, os)
  return real.map((block, index) => ({ ...block, id: `${assistant.value}:${index}`, shown: shown[index]!.code }))
})

function assistantLabel(id: Assistant) {
  return id === 'other' ? t('mcp.setup.other') : ASSISTANT_NAMES[id]
}

async function setEnabled(enabled: boolean) {
  busy.value = true
  try {
    await mcp.setEnabled(enabled)
  }
  finally {
    busy.value = false
  }
}

async function regenerate() {
  confirming.value = false
  busy.value = true
  try {
    await mcp.regenerateToken()
  }
  finally {
    busy.value = false
  }
}

async function copy(id: string, text: string) {
  try {
    await navigator.clipboard.writeText(text)
    copied.value = id
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = null
    }, COPIED_MS)
  }
  catch (err) {
    show(t('mcp.title'), t('mcp.copyFailed', { reason: err instanceof Error ? err.message : String(err) }))
  }
}

const EXAMPLES = ['streaming', 'meetings', 'tidy'] as const
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl" data-mcp-dialog>
      <DialogHeader class="border-b px-5 pt-5 pb-4">
        <DialogTitle class="flex items-center gap-2">
          <Sparkles class="size-4 text-sky-400" />
          {{ t('mcp.title') }}
        </DialogTitle>
        <DialogDescription>
          {{ t('mcp.description') }}
        </DialogDescription>
      </DialogHeader>

      <div class="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 py-4">
        <div class="flex flex-col gap-2 rounded-md border px-3 py-2.5">
          <label class="flex items-center justify-between gap-3 text-sm">
            <span class="font-medium">{{ t('mcp.enable') }}</span>
            <Switch
              :model-value="status?.enabled ?? false"
              :disabled="!status || busy"
              :aria-label="t('mcp.enable')"
              data-mcp-switch
              @update:model-value="setEnabled"
            />
          </label>
          <p class="flex min-w-0 items-center gap-2 text-xs" aria-live="polite" data-mcp-status :data-mcp-state="mcp.state.value">
            <span class="size-2 shrink-0 rounded-full" :class="dot" />
            <span class="min-w-0 break-words" :class="mcp.state.value === 'error' ? 'text-destructive' : 'text-muted-foreground'">{{ statusText }}</span>
          </p>
          <p v-if="lastUsed" class="pl-4 text-xs text-muted-foreground" data-mcp-last-used>
            {{ lastUsed }}
          </p>
        </div>

        <ul class="flex list-disc flex-col gap-1 pl-5 text-xs text-muted-foreground">
          <li>{{ t('mcp.about.local') }}</li>
          <li>{{ t('mcp.about.off') }}</li>
          <li>{{ t('mcp.about.rules') }}</li>
          <li>{{ t('mcp.about.secrets') }}</li>
          <li>{{ t('mcp.about.plugins') }}</li>
        </ul>

        <template v-if="connection">
          <section class="flex flex-col gap-2" data-mcp-setup>
            <h3 class="text-sm font-medium">
              {{ t('mcp.setup.title') }}
            </h3>
            <Tabs v-model="assistant" class="gap-3">
              <TabsList class="w-full">
                <TabsTrigger v-for="id in ASSISTANTS" :key="id" :value="id" class="text-xs" :data-mcp-assistant="id">
                  {{ assistantLabel(id) }}
                </TabsTrigger>
              </TabsList>
            </Tabs>
            <p class="text-xs text-muted-foreground">
              {{ t(`mcp.setup.hints.${assistant}`, { variable: TOKEN_ENV }) }}
            </p>
            <div v-for="block in blocks" :key="block.id" class="flex flex-col gap-1">
              <p v-if="blocks.length > 1 || block.kind === 'variable'" class="text-[11px] font-medium text-muted-foreground">
                {{ t(block.kind === 'variable' ? `mcp.setup.blocks.variable.${os === 'windows' ? 'windows' : 'unix'}` : `mcp.setup.blocks.${block.kind}`, { variable: TOKEN_ENV }) }}
              </p>
              <div class="relative">
                <pre class="rounded-md bg-neutral-900 p-3 pr-11 font-mono text-[11px] whitespace-pre-wrap text-neutral-300 [overflow-wrap:anywhere]" :data-mcp-snippet="block.kind">{{ block.shown }}</pre>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  class="absolute top-1.5 right-1.5"
                  :aria-label="t('mcp.copy')"
                  :title="t('mcp.copy')"
                  data-mcp-copy
                  @click="copy(block.id, block.code)"
                >
                  <Check v-if="copied === block.id" class="text-emerald-500" />
                  <Copy v-else />
                </Button>
              </div>
            </div>
          </section>
          <Collapsible v-model:open="tokenOpen" class="flex flex-col gap-1.5" data-mcp-token-section>
            <CollapsibleTrigger class="flex items-center gap-1.5 self-start text-xs text-muted-foreground hover:text-foreground" data-mcp-token-toggle>
              <ChevronRight class="size-3.5 transition-transform" :class="tokenOpen ? 'rotate-90' : ''" />
              {{ t('mcp.token.label') }}
            </CollapsibleTrigger>
            <CollapsibleContent class="flex flex-col gap-1.5">
              <div class="flex items-center gap-1.5">
                <div class="relative flex-1">
                  <KeyRound class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    :model-value="revealed ? connection.token : MASK"
                    readonly
                    class="h-8 pl-8 font-mono text-xs"
                    :aria-label="t('mcp.token.label')"
                    data-mcp-token
                  />
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :aria-label="revealed ? t('mcp.token.hide') : t('mcp.token.show')"
                  :title="revealed ? t('mcp.token.hide') : t('mcp.token.show')"
                  @click="revealed = !revealed"
                >
                  <EyeOff v-if="revealed" />
                  <Eye v-else />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :aria-label="t('mcp.copy')"
                  :title="t('mcp.copy')"
                  @click="copy('token', connection.token)"
                >
                  <Check v-if="copied === 'token'" class="text-emerald-500" />
                  <Copy v-else />
                </Button>
                <Button variant="outline" size="sm" :disabled="busy" data-mcp-regenerate @click="confirming = true">
                  <RefreshCw /> {{ t('mcp.token.regenerate') }}
                </Button>
              </div>
              <p class="text-[11px] text-muted-foreground">
                {{ t('mcp.token.hint') }}
              </p>
            </CollapsibleContent>
          </Collapsible>
        </template>
        <p v-else-if="status?.enabled" class="flex items-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle class="size-3.5 animate-spin" /> {{ t('mcp.status.starting') }}
        </p>

        <section class="flex flex-col gap-1.5">
          <h3 class="text-sm font-medium">
            {{ t('mcp.examples.title') }}
          </h3>
          <ul class="flex flex-col gap-1 text-xs text-muted-foreground">
            <li v-for="example in EXAMPLES" :key="example" class="rounded-md border border-neutral-800 px-3 py-1.5 italic">
              “{{ t(`mcp.examples.${example}`) }}”
            </li>
          </ul>
        </section>
      </div>

      <div class="flex justify-end rounded-b-xl border-t bg-muted/50 px-5 py-3">
        <Button variant="outline" size="sm" @click="open = false">
          {{ t('mcp.close') }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>

  <Dialog v-model:open="confirming">
    <DialogContent class="sm:max-w-md" data-mcp-regenerate-confirm>
      <DialogHeader>
        <DialogTitle>{{ t('mcp.token.confirmTitle') }}</DialogTitle>
        <DialogDescription class="leading-relaxed">
          {{ t('mcp.token.confirmBody') }}
        </DialogDescription>
      </DialogHeader>
      <DialogFooter>
        <Button variant="ghost" @click="confirming = false">
          {{ t('mcp.token.cancel') }}
        </Button>
        <Button @click="regenerate">
          {{ t('mcp.token.confirm') }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
