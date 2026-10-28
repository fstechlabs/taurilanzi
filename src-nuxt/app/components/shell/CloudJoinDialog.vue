<script setup lang="ts">
/** The first sync with an account that already has profiles: the user says what to do with this computer's. */
const { t } = useI18n()
const cloud = useCloud()

/** Left for later until the app opens again; the cloud settings keep offering it. */
const later = ref(false)
const open = computed({
  get: () => Boolean(cloud.status.value?.join) && !later.value,
  set: (value) => {
    if (!value) {
      later.value = true
    }
  },
})
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md" data-cloud-join>
      <DialogHeader>
        <DialogTitle>{{ t('cloudJoin.title') }}</DialogTitle>
        <DialogDescription>
          {{ t('cloudJoin.body', { count: cloud.status.value?.join?.cloud_profiles ?? 0 }) }}
        </DialogDescription>
      </DialogHeader>
      <ul class="flex flex-col gap-2 text-xs text-muted-foreground">
        <li>{{ t('cloudJoin.mergeHint') }}</li>
        <li>{{ t('cloudJoin.replaceHint') }}</li>
      </ul>
      <DialogFooter>
        <Button variant="outline" @click="cloud.join('replace')">
          {{ t('cloudJoin.replace') }}
        </Button>
        <Button @click="cloud.join('merge')">
          {{ t('cloudJoin.merge') }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
