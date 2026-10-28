<script setup lang="ts">
import { CloudDownload, ExternalLink, LogIn, LogOut, Pause, Play, RefreshCw, X } from '@lucide/vue'

/** Signing this computer in to the cloud, and how its sync is doing. */
const { t, locale } = useI18n()
const cloud = useCloud()
const status = cloud.status

const error = ref<string | null>(null)
const busy = ref(false)

async function attempt(work: () => Promise<unknown>) {
  error.value = null
  busy.value = true
  try {
    await work()
  }
  catch (err) {
    // Cancelling is the user's own doing, not a failure.
    if (!(err instanceof EngineError && err.kind === 'conflict')) {
      error.value = err instanceof Error ? err.message : String(err)
    }
  }
  finally {
    busy.value = false
  }
}

const lastSync = computed(() => {
  const at = status.value?.last_sync
  return at ? new Date(at).toLocaleString(locale.value, { dateStyle: 'medium', timeStyle: 'short' }) : null
})

const state = computed(() => {
  const value = status.value
  if (!value) {
    return ''
  }
  if (value.syncing) {
    return t('settings.cloud.syncing')
  }
  if (value.paused) {
    return t('settings.cloud.paused')
  }
  return lastSync.value ? t('settings.cloud.lastSync', { when: lastSync.value }) : t('settings.cloud.neverSynced')
})

</script>

<template>
  <div v-if="status" class="flex flex-col gap-6">
    <SettingsSection v-if="!status.signed_in" :title="t('settings.cloud.title')" :description="t('settings.cloud.about')">
      <SettingRow :label="t('settings.cloud.connect')" :hint="status.signing_in ? t('settings.cloud.waiting') : t('settings.cloud.connectHint')">
        <Button v-if="status.signing_in" variant="outline" size="sm" @click="cloud.cancelSignIn()">
          <X /> {{ t('settings.cloud.cancel') }}
        </Button>
        <Button v-else size="sm" :disabled="busy" data-cloud-sign-in @click="attempt(cloud.signIn)">
          <LogIn /> {{ t('settings.cloud.signIn') }}
        </Button>
      </SettingRow>
    </SettingsSection>

    <template v-else>
      <SettingsSection :title="t('settings.cloud.title')" :description="t('settings.cloud.aboutSignedIn')">
        <SettingRow :label="status.device_name" :hint="state">
          <Button variant="outline" size="sm" :disabled="status.paused || status.syncing" @click="cloud.syncNow()">
            <RefreshCw :class="status.syncing ? 'animate-spin' : ''" /> {{ t('settings.cloud.syncNow') }}
          </Button>
        </SettingRow>
        <p v-if="status.problem" class="py-3 text-xs text-destructive" role="alert">
          {{ status.problem }}
        </p>
        <SettingRow :label="status.paused ? t('settings.cloud.stopped') : t('settings.cloud.running')" :hint="status.paused ? t('settings.cloud.stoppedHint') : t('settings.cloud.runningHint')">
          <Button v-if="status.paused" size="sm" :disabled="busy" @click="attempt(() => cloud.setPaused(false))">
            <Play /> {{ t('settings.cloud.resume') }}
          </Button>
          <Button v-else variant="outline" size="sm" :disabled="busy" data-cloud-pause @click="attempt(() => cloud.setPaused(true))">
            <Pause /> {{ t('settings.cloud.stop') }}
          </Button>
        </SettingRow>
        <SettingRow :label="t('settings.cloud.versions')" :hint="t('settings.cloud.versionsHint')">
          <Button variant="outline" size="sm" @click="attempt(cloud.openWeb)">
            <ExternalLink /> {{ t('settings.cloud.openWeb') }}
          </Button>
        </SettingRow>
        <SettingRow :label="t('settings.cloud.signOut')" :hint="t('settings.cloud.signOutHint')">
          <Button variant="outline" size="sm" :disabled="busy" @click="attempt(cloud.signOut)">
            <LogOut /> {{ t('settings.cloud.signOutButton') }}
          </Button>
        </SettingRow>
      </SettingsSection>

      <SettingsSection v-if="status.join" :title="t('cloudJoin.title')" :description="t('cloudJoin.body', { count: status.join.cloud_profiles })">
        <div class="flex justify-end gap-2 py-3">
          <Button variant="outline" size="sm" @click="cloud.join('replace')">
            {{ t('cloudJoin.replace') }}
          </Button>
          <Button size="sm" @click="cloud.join('merge')">
            {{ t('cloudJoin.merge') }}
          </Button>
        </div>
      </SettingsSection>

      <SettingsSection v-if="status.elsewhere.length" :title="t('settings.cloud.elsewhere')" :description="t('settings.cloud.elsewhereHint')">
        <div v-for="profile in status.elsewhere" :key="profile.id" class="flex items-center gap-3 border-b py-3 last:border-b-0">
          <p class="min-w-0 flex-1 truncate text-sm">
            {{ profile.name }}
          </p>
          <Button variant="outline" size="sm" @click="attempt(() => cloud.bringBack(profile.id))">
            <CloudDownload /> {{ t('settings.cloud.bringBack') }}
          </Button>
        </div>
      </SettingsSection>
    </template>

    <p v-if="error" class="text-xs text-destructive" role="alert">
      {{ error }}
    </p>
  </div>
</template>
