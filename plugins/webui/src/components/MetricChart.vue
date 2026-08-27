<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  values: number[]
  color?: string
  formatValue?: (value: number) => string
  emptyText?: string
}>(), { color: '#18a058', emptyText: 'No samples yet' })

const width = 600
const height = 150
const max = computed(() => Math.max(...props.values, 1))
const points = computed(() => props.values.map((value, index) => {
  const x = props.values.length <= 1 ? 0 : (index / (props.values.length - 1)) * width
  const y = height - (value / max.value) * (height - 12)
  return `${x.toFixed(1)},${y.toFixed(1)}`
}).join(' '))
const latest = computed(() => props.values.at(-1) ?? 0)
const formattedLatest = computed(() => props.formatValue?.(latest.value) ?? String(latest.value))
</script>

<template>
  <div v-if="values.length" class="metric-chart">
    <div class="metric-value">{{ formattedLatest }}</div>
    <svg :viewBox="`0 0 ${width} ${height}`" role="img" aria-label="metric history">
      <line x1="0" :y1="height" :x2="width" :y2="height" stroke="currentColor" opacity="0.12" />
      <polyline :points="points" fill="none" :stroke="color" stroke-width="3" vector-effect="non-scaling-stroke" />
    </svg>
  </div>
  <div v-else class="metric-empty">{{ emptyText }}</div>
</template>

<style scoped>
.metric-chart { position: relative; min-height: 150px; }
.metric-chart svg { width: 100%; height: 150px; overflow: visible; }
.metric-value { position: absolute; right: 8px; top: 0; font-size: 20px; font-weight: 600; }
.metric-empty { height: 150px; display: grid; place-items: center; color: var(--n-text-color-3); }
</style>
