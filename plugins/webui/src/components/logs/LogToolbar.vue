<script setup lang="ts">
import { NButton, NInput, NInputNumber, NSelect, NSpace } from 'naive-ui'
import AppIcon from '@/components/icon/AppIcon.vue'
import { useI18n } from '@/i18n'
defineProps<{ profile: number | null; profileOptions: Array<{ label: string; value: number }>; lines: number; search: string; loading?: boolean }>()
const emit = defineEmits<{ 'update:profile': [number | null]; 'update:lines': [number]; 'update:search': [string]; refresh: []; clear: []; copy: []; download: [] }>()
const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-3 border-b border-border p-4 lg:flex-row lg:items-center lg:justify-between">
    <NSpace align="center" :wrap="true">
      <NSelect :value="profile" :options="profileOptions" :placeholder="t('logs.profile')" class="w-56" @update:value="emit('update:profile', $event)" />
      <NInput :value="search" clearable :placeholder="t('common.search')" class="w-56" @update:value="emit('update:search', $event)"><template #prefix><AppIcon name="search" :size="16" /></template></NInput>
      <NInputNumber :value="lines" :min="10" :max="1000" class="w-28" @update:value="value => emit('update:lines', value ?? 200)" />
    </NSpace>
    <NSpace align="center">
      <NButton quaternary aria-label="Copy" @click="emit('copy')"><template #icon><AppIcon name="copy" /></template></NButton>
      <NButton quaternary aria-label="Download" @click="emit('download')"><template #icon><AppIcon name="download" /></template></NButton>
      <NButton quaternary aria-label="Clear" @click="emit('clear')"><template #icon><AppIcon name="clear" /></template></NButton>
      <NButton type="primary" :loading="loading" @click="emit('refresh')"><template #icon><AppIcon name="refresh" /></template>{{ t('common.refresh') }}</NButton>
    </NSpace>
  </div>
</template>
