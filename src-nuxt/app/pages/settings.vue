<script setup lang="ts">
import { Crop as CropIcon, ImageUp, Play, Square } from '@lucide/vue'
import { commands } from '~/ipc/bindings'
import type { ImageDraft } from '~/composables/useKeyEditor'
import type { LanguageChoice, LockAction, Settings } from '~/types/engine'
import { SCENES } from '~/lib/scenes'

const { snapshot, screensaverUrl, assetUrl, upload } = useEngine()
const { preferences, device, failed } = useSettings()
const route = useRoute()
const router = useRouter()

const { t } = useI18n()

const TABS = ['general', 'dock', 'presses', 'new-keys', 'idle', 'profiles', 'integrations'] as const
const LANGUAGES: { value: LanguageChoice, label: string | null }[] = [
  { value: 'system', label: null },
  { value: 'es', label: 'Español' },
  { value: 'en', label: 'English' },
]
/** Kept in the address, so a reload stays on the same tab. */
const tab = computed({
  get: () => TABS.find(id => id === route.query.tab) ?? 'general',
  set: (id) => { router.replace({ query: { tab: id } }) },
})

const trying = computed(() => snapshot.value?.screen.mode === 'trial')
const onLock = computed({
  get: () => preferences.value?.screensaver.on_lock,
  set: (value) => {
    if (value && preferences.value) {
      preferences.value.screensaver.on_lock = value as LockAction
    }
  },
})

async function tryIt() {
  await call(commands.tryScreensaver(!trying.value))
}

type ChosenImage = Settings['screensaver']['image']
type CustomImage = Extract<ChosenImage, { type: 'custom' }>

const image = computed(() => preferences.value?.screensaver.image)
/** The last image of the user's, so it can be chosen again after trying one of the app's. */
const custom = ref<CustomImage | null>(null)
watch(image, (value) => {
  if (value?.type === 'custom') {
    custom.value = { ...value }
  }
}, { immediate: true })
const fileInput = ref<HTMLInputElement | null>(null)
const uploading = ref(false)
const editorOpen = ref(false)
const editing = ref<ImageDraft | null>(null)

function choose(next: ChosenImage) {
  if (preferences.value) {
    preferences.value.screensaver.image = next
  }
}

function chooseCustom() {
  if (custom.value) {
    choose(custom.value)
  }
  else {
    fileInput.value?.click()
  }
}

/** Opens the editor on the original image, framed as it was. */
function edit() {
  const current = custom.value
  if (!current) {
    return
  }
  editing.value = {
    asset: current.asset,
    source: current.source ?? current.asset,
    crop: current.crop ?? null,
    fit: 'cover',
    inset: 0,
    tint: null,
    icon: null,
  }
  editorOpen.value = true
}

function applyEdit(draft: ImageDraft) {
  custom.value = { type: 'custom', asset: draft.asset, source: draft.source, crop: draft.crop }
  choose(custom.value)
}

async function uploadImage(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) {
    return
  }
  uploading.value = true
  try {
    const asset = await upload(file, file.type)
    custom.value = { type: 'custom', asset, source: asset, crop: null }
    choose(custom.value)
    edit()
  }
  finally {
    uploading.value = false
  }
}
</script>

<template>
  <main class="min-w-0 flex-1 overflow-y-auto">
    <div v-if="preferences && device" class="mx-auto flex max-w-3xl flex-col gap-6 px-8 py-8">
      <div class="flex items-baseline justify-between">
        <h1 class="text-lg font-semibold">
          {{ t('settings.title') }}
        </h1>
        <span v-if="failed" class="text-xs text-destructive">{{ t('settings.saveFailed') }}</span>
      </div>

      <Tabs v-model="tab" class="gap-6">
        <TabsList variant="line" class="border-b border-neutral-800">
          <TabsTrigger v-for="id in TABS" :key="id" :value="id">
            {{ t(`settings.tabs.${id}`) }}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="general">
          <SettingsSection>
            <SettingRow :label="t('settings.general.language')" :hint="t('settings.general.languageHint')">
              <Select v-model="preferences.language">
                <SelectTrigger class="w-44" :aria-label="t('settings.general.language')">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="option in LANGUAGES" :key="option.value" :value="option.value">
                    {{ option.label ?? t('settings.general.system') }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </SettingRow>
          </SettingsSection>
        </TabsContent>

        <TabsContent value="dock">
          <SettingsSection>
            <SettingRow :label="t('settings.dock.brightness')">
              <SliderField v-model="device.brightness" :label="t('settings.dock.brightnessLabel')" :min="0" :max="100" unit=" %" />
            </SettingRow>
            <SettingRow :label="t('settings.dock.sleep')" :hint="t('settings.dock.sleepHint')">
              <MinutesSelect v-model="device.sleep_minutes" :label="t('settings.dock.sleepLabel')" :options="[0, 1, 5, 10, 15, 30, 60]" />
            </SettingRow>
          </SettingsSection>
        </TabsContent>

        <TabsContent value="presses">
          <SettingsSection>
            <SettingRow :label="t('settings.presses.long')" :hint="t('settings.presses.longHint')">
              <SliderField v-model="preferences.gestures.long_press_ms" :label="t('settings.presses.longLabel')" :min="250" :max="2000" :step="50" unit=" ms" />
            </SettingRow>
            <SettingRow :label="t('settings.presses.double')" :hint="t('settings.presses.doubleHint')">
              <SliderField v-model="preferences.gestures.double_press_ms" :label="t('settings.presses.doubleLabel')" :min="150" :max="800" :step="25" unit=" ms" />
            </SettingRow>
          </SettingsSection>
        </TabsContent>

        <TabsContent value="new-keys">
          <SettingsSection :description="t('settings.newKeys.description')">
            <NewKeyFields v-model="preferences.new_keys" />
          </SettingsSection>
        </TabsContent>

        <TabsContent value="idle">
          <SettingsSection>
            <SettingRow :label="t('settings.idle.onLock')">
              <ToggleGroup v-model="onLock" type="single" variant="outline" size="sm" :aria-label="t('settings.idle.onLockLabel')">
                <ToggleGroupItem value="screensaver">
                  {{ t('settings.idle.screensaver') }}
                </ToggleGroupItem>
                <ToggleGroupItem value="off">
                  {{ t('settings.idle.off') }}
                </ToggleGroupItem>
                <ToggleGroupItem value="keep">
                  {{ t('settings.idle.keep') }}
                </ToggleGroupItem>
              </ToggleGroup>
            </SettingRow>
            <SettingRow :label="t('settings.idle.idle')" :hint="t('settings.idle.idleHint')">
              <MinutesSelect v-model="preferences.screensaver.idle_minutes" :label="t('settings.idle.idleLabel')" :options="[0, 1, 2, 5, 10, 15, 30, 60]" />
            </SettingRow>
            <SettingRow :label="t('settings.idle.offAfter')" :hint="t('settings.idle.offAfterHint')">
              <MinutesSelect v-model="preferences.screensaver.off_after_minutes" :label="t('settings.idle.offAfterLabel')" :options="[0, 5, 10, 15, 30, 60, 120]" />
            </SettingRow>
            <SettingRow :label="t('settings.idle.brightness')">
              <SliderField v-model="preferences.screensaver.brightness" :label="t('settings.idle.brightness')" :min="5" :max="100" :step="5" unit=" %" />
            </SettingRow>
            <div class="flex flex-col gap-3 py-4">
              <div class="flex items-center justify-between gap-6">
                <div>
                  <p class="text-sm">
                    {{ t('settings.idle.image') }}
                  </p>
                  <p class="text-xs text-muted-foreground">
                    {{ t('settings.idle.imageHint') }}
                  </p>
                </div>
                <div class="flex shrink-0 gap-2">
                  <Button v-if="image?.type === 'custom'" variant="outline" size="sm" @click="edit">
                    <CropIcon /> {{ t('settings.idle.editImage') }}
                  </Button>
                  <Button variant="outline" size="sm" :disabled="uploading" @click="fileInput?.click()">
                    <ImageUp /> {{ t('settings.idle.uploadImage') }}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    :disabled="!snapshot?.connected"
                    :title="snapshot?.connected ? undefined : t('settings.idle.connectToTry')"
                    @click="tryIt"
                  >
                    <template v-if="trying">
                      <Square /> {{ t('settings.idle.stopTrial') }}
                    </template>
                    <template v-else>
                      <Play /> {{ t('settings.idle.tryOnDock') }}
                    </template>
                  </Button>
                </div>
              </div>
              <input
                ref="fileInput"
                type="file"
                accept="image/png,image/jpeg,image/webp,image/svg+xml"
                class="hidden"
                :aria-label="t('settings.idle.imageLabel')"
                @change="uploadImage"
              >
              <div class="grid grid-cols-4 gap-3" role="group" :aria-label="t('settings.idle.images')">
                <SceneCard
                  v-for="scene in SCENES"
                  :key="scene.id"
                  :src="screensaverUrl(scene.id, 360)"
                  :name="scene.name"
                  :selected="image?.type === 'scene' && image.scene === scene.id"
                  @select="choose({ type: 'scene', scene: scene.id })"
                />
                <SceneCard
                  :src="custom ? assetUrl(custom.asset) : null"
                  :name="t('settings.idle.yourImage')"
                  :selected="image?.type === 'custom'"
                  @select="chooseCustom"
                >
                  <ImageUp class="size-5" />
                </SceneCard>
              </div>
            </div>
          </SettingsSection>
        </TabsContent>

        <TabsContent value="profiles">
          <ProfileSettings v-model="preferences.profiles" />
        </TabsContent>

        <TabsContent value="integrations">
          <IntegrationsSettings />
        </TabsContent>
      </Tabs>
    </div>
    <ImageEditorDialog
      v-if="editing"
      v-model:open="editorOpen"
      :image="editing"
      shape="screensaver"
      background="#000000"
      @update:image="applyEdit"
    />
  </main>
</template>
