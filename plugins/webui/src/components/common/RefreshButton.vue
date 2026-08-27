<script setup lang="ts">
import { NButton } from 'naive-ui'
import AppIcon from '@/components/icon/AppIcon.vue'
withDefaults(defineProps<{ loading?: boolean; label?: string }>(), { label: '刷新' })
defineEmits<{ click: [] }>()
</script>

<template>
  <NButton secondary :loading="loading" @click="$emit('click')">
    <template #icon><AppIcon name="refresh" /></template>{{ label }}
  </NButton>
</template>
