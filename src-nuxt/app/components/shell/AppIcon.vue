<script setup lang="ts">
import { AppWindow } from '@lucide/vue'

const props = defineProps<{ appId: string, small?: boolean }>()
const { iconUrl } = useProfiles()

/** Apps no longer installed have no icon. */
const failed = ref(false)
watch(() => props.appId, () => {
  failed.value = false
})
</script>

<template>
  <AppWindow v-if="failed" class="shrink-0 text-muted-foreground" :class="props.small ? 'size-4' : 'size-5'" />
  <img
    v-else
    :src="iconUrl(props.appId)"
    alt=""
    class="shrink-0"
    :class="props.small ? 'size-4' : 'size-5'"
    draggable="false"
    @error="failed = true"
  >
</template>
