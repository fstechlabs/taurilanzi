<script setup lang="ts">
import { ChevronDown, LoaderCircle, Plus, Send, X } from '@lucide/vue'
import { commands } from '~/ipc/bindings'
import type { BodyKind } from '~/lib/actions/http'
import { COMMON_HEADERS, HTTP_METHODS, bodyOf, httpError, jsonError, prettyJson } from '~/lib/actions/http'
import type { HttpBody, HttpPair, HttpRequest, HttpResponse } from '~/types/engine'

const request = defineModel<HttpRequest>({ required: true })
const { t } = useI18n()

const BODY_KINDS: BodyKind[] = ['none', 'json', 'text', 'form']
/** One datalist per field set, so two requests open at once do not share ids. */
const listId = `http-headers-${useId()}`

const headers = computed(() => request.value.headers ?? [])
const body = computed<HttpBody>(() => request.value.body ?? { type: 'none' })
const problem = computed(() => httpError(request.value))
const bodyless = computed(() => ['GET', 'HEAD'].includes(request.value.method) && body.value.type !== 'none')

function update(changes: Partial<HttpRequest>) {
  request.value = { ...request.value, ...changes }
}

function editPair(list: HttpPair[], index: number, changes: Partial<HttpPair>) {
  return list.map((pair, i) => (i === index ? { ...pair, ...changes } : pair))
}

function setHeader(index: number, changes: Partial<HttpPair>) {
  update({ headers: editPair(headers.value, index, changes) })
}

function setBody(next: HttpBody) {
  update({ body: next })
}

function setBodyKind(kind: unknown) {
  if (typeof kind === 'string' && kind !== body.value.type) {
    setBody(bodyOf(kind as BodyKind, body.value))
  }
}

const fields = computed(() => (body.value.type === 'form' ? body.value.fields : []))

function setField(index: number, changes: Partial<HttpPair>) {
  setBody({ type: 'form', fields: editPair(fields.value, index, changes) })
}

const seconds = computed(() => Math.round((request.value.timeout_ms ?? 10000) / 1000))

function setSeconds(value: string | number) {
  const parsed = Math.round(Number(value))
  if (Number.isFinite(parsed) && parsed >= 1) {
    update({ timeout_ms: Math.min(120, parsed) * 1000 })
  }
}

const trying = ref(false)
const answer = ref<HttpResponse | null>(null)
const failure = ref<string | null>(null)

async function tryIt() {
  trying.value = true
  answer.value = null
  failure.value = null
  try {
    answer.value = await call(commands.tryHttp(toRaw(request.value)))
  }
  catch (err) {
    failure.value = err instanceof Error ? err.message : String(err)
  }
  finally {
    trying.value = false
  }
}

const answerBody = computed(() => {
  const value = answer.value
  if (!value) {
    return ''
  }
  return value.content_type?.includes('json') ? prettyJson(value.body) : value.body
})

const statusClass = computed(() => {
  const status = answer.value?.status ?? 0
  if (status < 300) {
    return 'bg-emerald-500/15 text-emerald-300'
  }
  return status < 400 ? 'bg-amber-500/15 text-amber-300' : 'bg-red-500/15 text-red-300'
})
</script>

<template>
  <div class="flex flex-col gap-3">
    <div class="flex gap-2">
      <Select :model-value="request.method" @update:model-value="v => typeof v === 'string' && update({ method: v as HttpRequest['method'] })">
        <SelectTrigger class="w-28 shrink-0 font-mono text-xs" :aria-label="t('actions.http.method')">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="method in HTTP_METHODS" :key="method" :value="method" class="font-mono text-xs">
            {{ method }}
          </SelectItem>
        </SelectContent>
      </Select>
      <Input
        :model-value="request.url"
        placeholder="https://…"
        class="font-mono text-xs"
        :aria-label="t('actions.http.url')"
        @update:model-value="update({ url: String($event) })"
      />
    </div>

    <div class="flex flex-col gap-1.5">
      <div class="flex items-center justify-between">
        <Label class="text-xs text-muted-foreground">{{ t('actions.http.headers') }}</Label>
        <Button variant="ghost" size="sm" class="h-7" @click="update({ headers: [...headers, { name: '', value: '' }] })">
          <Plus /> {{ t('actions.http.addHeader') }}
        </Button>
      </div>
      <p v-if="!headers.length" class="text-[11px] text-muted-foreground">
        {{ t('actions.http.noHeaders') }}
      </p>
      <div v-for="(header, i) in headers" :key="i" class="flex gap-1.5">
        <Input
          :model-value="header.name"
          :list="listId"
          :placeholder="t('actions.http.name')"
          class="w-36 shrink-0 font-mono text-xs"
          :aria-label="t('actions.http.headerName', { n: i + 1 })"
          @update:model-value="setHeader(i, { name: String($event) })"
        />
        <Input
          :model-value="header.value"
          :placeholder="t('actions.http.value')"
          class="font-mono text-xs"
          :aria-label="t('actions.http.headerValue', { n: i + 1 })"
          @update:model-value="setHeader(i, { value: String($event) })"
        />
        <Button variant="ghost" size="icon-sm" class="shrink-0" :aria-label="t('actions.http.removeHeader', { n: i + 1 })" @click="update({ headers: headers.filter((_, j) => j !== i) })">
          <X />
        </Button>
      </div>
      <datalist :id="listId">
        <option v-for="name in COMMON_HEADERS" :key="name" :value="name" />
      </datalist>
    </div>

    <div class="flex flex-col gap-1.5">
      <Label class="text-xs text-muted-foreground">{{ t('actions.http.body') }}</Label>
      <ToggleGroup :model-value="body.type" type="single" variant="outline" size="sm" class="w-full" :aria-label="t('actions.http.bodyKind')" @update:model-value="setBodyKind">
        <ToggleGroupItem v-for="kind in BODY_KINDS" :key="kind" :value="kind" class="flex-1">
          {{ t(`actions.http.bodyKinds.${kind}`) }}
        </ToggleGroupItem>
      </ToggleGroup>

      <template v-if="body.type === 'json'">
        <Textarea
          :model-value="body.json"
          rows="6"
          spellcheck="false"
          class="font-mono text-xs"
          :aria-label="t('actions.http.jsonBody')"
          @update:model-value="setBody({ type: 'json', json: String($event) })"
        />
        <div class="flex items-center justify-between gap-2">
          <p class="min-w-0 truncate text-[11px]" :class="jsonError(body.json) ? 'text-destructive' : 'text-muted-foreground'">
            {{ jsonError(body.json) ? t('actions.http.invalidJson', { error: jsonError(body.json) }) : t('actions.http.jsonHint') }}
          </p>
          <Button variant="ghost" size="sm" class="h-7 shrink-0" :disabled="Boolean(jsonError(body.json))" @click="setBody({ type: 'json', json: prettyJson(body.json) })">
            {{ t('actions.http.format') }}
          </Button>
        </div>
      </template>

      <template v-else-if="body.type === 'text'">
        <Input
          :model-value="body.content_type"
          placeholder="text/plain"
          class="font-mono text-xs"
          :aria-label="t('actions.http.textType')"
          @update:model-value="setBody({ ...body, content_type: String($event) } as HttpBody)"
        />
        <Textarea
          :model-value="body.text"
          rows="5"
          spellcheck="false"
          class="font-mono text-xs"
          :aria-label="t('actions.http.textBody')"
          @update:model-value="setBody({ ...body, text: String($event) } as HttpBody)"
        />
      </template>

      <template v-else-if="body.type === 'form'">
        <div v-for="(field, i) in fields" :key="i" class="flex gap-1.5">
          <Input
            :model-value="field.name"
            :placeholder="t('actions.http.field')"
            class="w-36 shrink-0 font-mono text-xs"
            :aria-label="t('actions.http.fieldName', { n: i + 1 })"
            @update:model-value="setField(i, { name: String($event) })"
          />
          <Input
            :model-value="field.value"
            :placeholder="t('actions.http.value')"
            class="font-mono text-xs"
            :aria-label="t('actions.http.fieldValue', { n: i + 1 })"
            @update:model-value="setField(i, { value: String($event) })"
          />
          <Button variant="ghost" size="icon-sm" class="shrink-0" :aria-label="t('actions.http.removeField', { n: i + 1 })" @click="setBody({ type: 'form', fields: fields.filter((_, j) => j !== i) })">
            <X />
          </Button>
        </div>
        <Button variant="ghost" size="sm" class="h-7 self-start" @click="setBody({ type: 'form', fields: [...fields, { name: '', value: '' }] })">
          <Plus /> {{ t('actions.http.addField') }}
        </Button>
      </template>

      <p v-if="bodyless" class="text-[11px] text-amber-300">
        {{ t('actions.http.bodyless', { method: request.method }) }}
      </p>
    </div>

    <label class="flex items-center justify-between gap-2 text-xs">
      <span>{{ t('actions.http.showResult') }} <span class="block text-[11px] text-muted-foreground">{{ t('actions.http.showResultHint') }}</span></span>
      <Switch :model-value="request.show_result ?? true" :aria-label="t('actions.http.showResult')" @update:model-value="update({ show_result: $event })" />
    </label>

    <Collapsible>
      <CollapsibleTrigger class="group flex w-full items-center justify-between text-xs text-muted-foreground hover:text-foreground">
        {{ t('actions.http.more') }} <ChevronDown class="size-3.5 transition-transform group-data-[state=open]:rotate-180" />
      </CollapsibleTrigger>
      <CollapsibleContent class="flex flex-col gap-3 pt-3">
        <div class="flex items-center justify-between gap-2 text-xs">
          {{ t('actions.http.timeout') }}
          <div class="flex items-center gap-1">
            <Input
              :model-value="seconds"
              type="number"
              min="1"
              max="120"
              class="h-8 w-20 text-right"
              :aria-label="t('actions.http.timeoutLabel')"
              @update:model-value="setSeconds"
            />
            <span class="text-muted-foreground">s</span>
          </div>
        </div>
        <label class="flex items-center justify-between gap-2 text-xs">
          <span>{{ t('actions.http.insecure') }} <span class="block text-[11px] text-muted-foreground">{{ t('actions.http.insecureHint') }}</span></span>
          <Switch :model-value="Boolean(request.insecure)" :aria-label="t('actions.http.insecure')" @update:model-value="update({ insecure: $event })" />
        </label>
      </CollapsibleContent>
    </Collapsible>

    <div class="flex flex-col gap-2">
      <Button variant="outline" size="sm" class="self-start" :disabled="trying || Boolean(problem)" :title="problem ?? t('actions.http.tryHint')" @click="tryIt">
        <LoaderCircle v-if="trying" class="animate-spin" />
        <Send v-else />
        {{ t('actions.http.try') }}
      </Button>
      <p v-if="failure" class="text-xs text-destructive" data-http-error>
        {{ t('actions.http.failed', { error: failure }) }}
      </p>
      <div v-if="answer" class="flex flex-col gap-1.5 rounded-md border bg-neutral-950/60 p-2" data-http-answer>
        <div class="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
          <span class="rounded px-1.5 py-0.5 font-mono font-semibold" :class="statusClass">{{ answer.status }} {{ answer.reason }}</span>
          <span>{{ answer.duration_ms }} ms</span>
          <span v-if="answer.content_type" class="truncate">· {{ answer.content_type }}</span>
        </div>
        <pre v-if="answerBody" class="max-h-56 overflow-auto font-mono text-[11px] break-all whitespace-pre-wrap">{{ answerBody }}</pre>
        <p v-else class="text-[11px] text-muted-foreground">
          {{ t('actions.http.noContent') }}
        </p>
        <p v-if="answer.truncated" class="text-[11px] text-muted-foreground">
          {{ t('actions.http.truncated') }}
        </p>
      </div>
    </div>
  </div>
</template>
