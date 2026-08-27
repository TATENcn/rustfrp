<script setup lang="ts">
import { NAlert, NButton, NEmpty, NResult, NSkeleton, NSpace } from 'naive-ui'
import { useI18n } from '@/i18n'
defineProps<{ phase: string; error?: string | null; empty?: boolean; emptyText?: string }>()
defineEmits<{ retry: [] }>()
const { locale } = useI18n()
</script>

<template>
  <div v-if="phase === 'loading'" class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
    <div v-for="index in 4" :key="index" class="rounded-card border border-border bg-surface p-5 shadow-card">
      <NSkeleton text width="45%" /><NSkeleton class="mt-4" text width="70%" :height="30" />
    </div>
  </div>
  <NResult v-else-if="phase === 'error'" status="error" :title="locale === 'zh' ? '加载失败' : 'Failed to load'" :description="error ?? (locale === 'zh' ? '无法获取数据' : 'Unable to fetch data')">
    <template #footer><NButton type="primary" @click="$emit('retry')">{{ locale === 'zh' ? '重试' : 'Retry' }}</NButton></template>
  </NResult>
  <NSpace v-else vertical :size="16">
    <NAlert v-if="phase === 'stale'" type="warning" :title="error ?? (locale === 'zh' ? '数据暂时无法刷新' : 'Data could not be refreshed')">{{ locale === 'zh' ? '当前展示的是最近一次成功获取的数据。' : 'Showing the most recently available data.' }}</NAlert>
    <NEmpty v-if="empty" :description="emptyText ?? (locale === 'zh' ? '暂无数据' : 'No data')" />
    <slot v-else />
  </NSpace>
</template>
