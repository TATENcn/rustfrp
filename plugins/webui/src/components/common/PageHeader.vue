<script setup lang="ts">
defineProps<{ title: string; description?: string }>()
</script>

<template>
  <header class="mb-6 flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
    <div class="min-w-0">
      <div class="flex items-center gap-3">
        <slot name="icon" />
        <h1 class="m-0 text-2xl font-semibold tracking-tight text-foreground">{{ title }}</h1>
        <slot name="status" />
      </div>
      <p v-if="description" class="mt-1.5 mb-0 max-w-3xl text-sm leading-6 text-foreground-muted">{{ description }}</p>
    </div>
    <div class="flex shrink-0 flex-wrap items-center gap-2"><slot name="actions" /></div>
  </header>
</template>
