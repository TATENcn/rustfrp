<script setup lang="ts">
import { computed } from 'vue'
import { NButton } from 'naive-ui'
import AppIcon from '@/components/icon/AppIcon.vue'
import { useI18n } from '@/i18n'

const props = defineProps<{ loading?: boolean; label?: string }>()
defineEmits<{ click: [] }>()
const { t } = useI18n()
const buttonLabel = computed(() => props.label ?? t('common.refresh'))
</script>

<template>
  <NButton secondary :loading="loading" :aria-label="buttonLabel" @click="$emit('click')">
    <template #icon><AppIcon name="refresh" /></template>{{ buttonLabel }}
  </NButton>
</template>
