<script setup lang="ts">
import { Pencil } from '@lucide/vue'
import type { Settings } from '~/types/engine'

const switching = defineModel<Settings['profiles']>({ required: true })
const { profiles } = useProfiles()
const { t } = useI18n()

const linking = ref<string | null>(null)
const linkingOpen = computed({
  get: () => linking.value !== null,
  set: (value) => {
    if (!value) {
      linking.value = null
    }
  },
})
</script>

<template>
  <div class="flex flex-col gap-6">
    <SettingsSection>
      <SettingRow
        :label="t('settings.profiles.follow')"
        :hint="t('settings.profiles.followHint')"
      >
        <Switch v-model="switching.follow_apps" :aria-label="t('settings.profiles.follow')" />
      </SettingRow>
    </SettingsSection>

    <SettingsSection :title="t('settings.profiles.linkedApps')" :description="t('settings.profiles.linkedAppsHint')">
      <div
        v-for="profile in profiles"
        :key="profile.id"
        class="flex min-h-12 items-center justify-between gap-6 py-2"
      >
        <div class="flex min-w-0 flex-col gap-1">
          <p class="text-sm">
            {{ profile.name }}
          </p>
          <div v-if="profile.apps.length" class="flex flex-wrap gap-x-3 gap-y-1">
            <span v-for="app in profile.apps" :key="app.id" class="flex items-center gap-1.5 text-xs text-muted-foreground">
              <AppIcon :app-id="app.id" class="size-4!" />
              {{ app.name }}
            </span>
          </div>
          <p v-else class="text-xs text-muted-foreground">
            {{ t('settings.profiles.noApps') }}
          </p>
        </div>
        <Button variant="outline" size="sm" class="shrink-0" @click="linking = profile.id">
          <Pencil /> {{ t('settings.profiles.edit') }}
        </Button>
      </div>
    </SettingsSection>

    <LinkedAppsDialog v-if="linking" v-model:open="linkingOpen" :profile-id="linking" />
  </div>
</template>
