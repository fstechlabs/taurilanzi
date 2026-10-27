<script setup lang="ts">
import { open as choose } from '@tauri-apps/plugin-dialog'
import { AppWindow, File, Folder } from '@lucide/vue'
import { APP_FILES, fileTitle, isAppFile, os, parentFolder } from '~/lib/os'

const path = defineModel<string>({ required: true })
const props = defineProps<{
  /** Only apps can be chosen, for closing one. */
  appsOnly?: boolean
}>()

const { t } = useI18n()
const { appAt } = useProfiles()

/** Launcher entries on Linux are named after the app's id; the entry has its real name. */
const entryName = ref<string | null>(null)
watch(path, async (value) => {
  entryName.value = null
  if (os === 'linux' && isAppFile(value)) {
    entryName.value = await appAt(value).then(app => app.name, () => null)
  }
}, { immediate: true })

const name = computed(() => entryName.value ?? fileTitle(path.value))
const folder = computed(() => parentFolder(path.value))

async function pick(kind: 'app' | 'file' | 'folder') {
  const chosen = await choose({
    title: t(kind === 'app' ? 'actions.path.chooseApp' : kind === 'file' ? 'actions.path.chooseFile' : 'actions.path.chooseFolder'),
    directory: kind === 'folder',
    defaultPath: kind === 'app' ? APP_FILES.folder : undefined,
    filters: kind === 'app' ? [{ name: t('actions.path.apps'), extensions: APP_FILES.extensions }] : undefined,
  })
  if (typeof chosen === 'string') {
    path.value = chosen
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <div v-if="path" class="flex min-w-0 flex-col rounded-md border px-3 py-1.5" :title="path">
      <span class="truncate text-sm">{{ name }}</span>
      <span class="truncate text-[11px] text-muted-foreground">{{ folder }}</span>
    </div>
    <p v-else class="text-xs text-muted-foreground">
      {{ t(props.appsOnly ? 'actions.path.closeHint' : 'actions.path.openHint') }}
    </p>
    <div class="flex flex-wrap gap-1.5">
      <Button variant="outline" size="sm" @click="pick('app')">
        <AppWindow /> {{ t(path && props.appsOnly ? 'actions.path.changeApp' : 'actions.path.app') }}
      </Button>
      <template v-if="!props.appsOnly">
        <Button variant="outline" size="sm" @click="pick('file')">
          <File /> {{ t('actions.path.file') }}
        </Button>
        <Button variant="outline" size="sm" @click="pick('folder')">
          <Folder /> {{ t('actions.path.folder') }}
        </Button>
      </template>
    </div>
  </div>
</template>
