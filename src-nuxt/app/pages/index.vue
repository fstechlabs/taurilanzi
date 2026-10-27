<script setup lang="ts">
const route = useRoute()
const { snapshot } = useEngine()
const { selection, select, focus } = useWorkspace()

// `?key=N` selects key N of the home page, for links and tests.
const initial = Number(route.query.key)
watch(() => snapshot.value?.home_page, (home) => {
  if (home && Number.isInteger(initial) && !selection.value) {
    select(home, initial)
    focus(home)
  }
}, { immediate: true })
</script>

<template>
  <main class="relative min-w-0 flex-1">
    <PageCanvas />
  </main>
  <KeyInspector :target="selection" />
  <KeyDragGhost />
</template>
