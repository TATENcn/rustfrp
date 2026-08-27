<script setup lang="ts">
import { NCard, NProgress } from 'naive-ui'
import AppIcon from '@/components/icon/AppIcon.vue'
import type { AppIconName } from '@/components/icon/types'
defineProps<{ label: string; value: string | number; icon: AppIconName; accent?: string; detail?: string; percentage?: number }>()
</script>

<template>
  <NCard size="small" class="metric-card" :bordered="false">
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <div class="text-sm font-medium text-foreground-muted">{{ label }}</div>
        <div class="mt-2 truncate text-2xl font-semibold tracking-tight text-foreground">{{ value }}</div>
        <div v-if="detail" class="mt-1 text-xs text-foreground-muted">{{ detail }}</div>
      </div>
      <span class="grid size-10 shrink-0 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon :name="icon" :size="20" /></span>
    </div>
    <NProgress v-if="percentage !== undefined" class="mt-4" type="line" :percentage="percentage" :show-indicator="false" :height="6" />
  </NCard>
</template>

<style scoped>
.metric-card { background: var(--ui-surface); box-shadow: var(--shadow-card); }
</style>
