<script setup lang="ts">
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import AppIcon from '@/components/icon/AppIcon.vue'
import type { AppIconName } from '@/components/icon/types'

type StatusKind = 'running' | 'live' | 'healthy' | 'connecting' | 'refreshing' | 'stopped' | 'idle' | 'reconnecting' | 'stale' | 'failed' | 'offline' | 'error'
const props = withDefaults(defineProps<{ status: StatusKind; label?: string; size?: 'small' | 'medium' }>(), { size: 'small' })
const type = computed(() => ['running', 'live', 'healthy'].includes(props.status) ? 'success' : ['connecting', 'refreshing'].includes(props.status) ? 'info' : ['reconnecting', 'stale'].includes(props.status) ? 'warning' : ['failed', 'offline', 'error'].includes(props.status) ? 'error' : 'default')
const icon = computed<AppIconName>(() => type.value === 'success' ? 'running' : type.value === 'warning' ? 'warning' : type.value === 'error' ? 'error' : props.status === 'connecting' || props.status === 'refreshing' ? 'refresh' : 'stopped')
</script>

<template>
  <NTag :type="type" :size="size" round :bordered="false">
    <template #icon><AppIcon :name="icon" :size="14" /></template>
    {{ label ?? status }}
  </NTag>
</template>
