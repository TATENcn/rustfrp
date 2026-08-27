<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import AppIcon from '@/components/icon/AppIcon.vue'
import { useI18n } from '@/i18n'

const props = defineProps<{ value: number | null; stale?: boolean }>()
const { locale } = useI18n()
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null
onMounted(() => { timer = setInterval(() => { now.value = Date.now() }, 15_000) })
onUnmounted(() => { if (timer) clearInterval(timer) })
const label = computed(() => {
  if (!props.value) return '—'
  const seconds = Math.max(0, Math.floor((now.value - props.value) / 1000))
  if (seconds < 10) return locale.value === 'zh' ? '刚刚更新' : 'Updated just now'
  if (seconds < 60) return locale.value === 'zh' ? `${seconds} 秒前更新` : `Updated ${seconds}s ago`
  const minutes = Math.floor(seconds / 60)
  return minutes < 60 ? (locale.value === 'zh' ? `${minutes} 分钟前更新` : `Updated ${minutes}m ago`) : new Date(props.value).toLocaleTimeString()
})
</script>

<template>
  <span class="inline-flex items-center gap-1.5 text-xs" :class="stale ? 'text-warning' : 'text-foreground-muted'">
    <AppIcon name="clock" :size="13" />{{ label }}
  </span>
</template>
