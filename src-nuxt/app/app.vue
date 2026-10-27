<script setup lang="ts">
const { locale } = useI18n()
const { name } = useEngine()

useHead({
  title: () => name.value ?? '',
  htmlAttrs: { lang: locale, class: 'dark' },
})
</script>

<template>
  <TooltipProvider>
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </TooltipProvider>
</template>
