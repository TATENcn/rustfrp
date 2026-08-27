<script setup lang="ts">
import { computed } from 'vue'
import { NIcon } from 'naive-ui'
import { appIcons } from './registry'
import type { AppIconName } from './types'

const props = withDefaults(defineProps<{ name: AppIconName; size?: number; color?: string; label?: string }>(), { size: 18 })
const icon = computed(() => appIcons[props.name])
</script>

<template>
  <NIcon :size="size" :color="color" :aria-label="label" :aria-hidden="label ? undefined : true">
    <component :is="icon" />
  </NIcon>
</template>
