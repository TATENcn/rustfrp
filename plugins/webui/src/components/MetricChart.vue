<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { LineChart } from 'echarts/charts'
import { GridComponent, TooltipComponent } from 'echarts/components'
import { init, use, type ECharts, type EChartsCoreOption } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { useThemeStore } from '@/stores/theme'

use([LineChart, GridComponent, TooltipComponent, CanvasRenderer])

const props = withDefaults(defineProps<{
  values: number[]
  timestamps?: string[]
  color?: string
  formatValue?: (value: number) => string
  emptyText?: string
}>(), { color: '#18a058', timestamps: () => [], emptyText: 'No samples yet' })

const themeStore = useThemeStore()
const chartElement = ref<HTMLDivElement | null>(null)
const latest = computed(() => props.values.at(-1) ?? 0)
const formattedLatest = computed(() => props.formatValue?.(latest.value) ?? String(latest.value))
let chart: ECharts | null = null
let resizeObserver: ResizeObserver | null = null

function cssColor(value: string): string {
  const match = value.match(/^var\((--[^)]+)\)$/)
  return match ? getComputedStyle(document.documentElement).getPropertyValue(match[1]).trim() || value : value
}

function sampleTime(index: number): number {
  const timestamp = props.timestamps[index]
  if (timestamp) {
    const parsed = Date.parse(timestamp)
    if (Number.isFinite(parsed)) return parsed
  }
  return Date.now() - (props.values.length - 1 - index) * 3_000
}

function renderChart() {
  if (!chartElement.value || !props.values.length) return
  chart ??= init(chartElement.value, undefined, { renderer: 'canvas' })

  const styles = getComputedStyle(document.documentElement)
  const foreground = styles.getPropertyValue('--ui-foreground').trim() || (themeStore.isDark ? '#e5e7eb' : '#111827')
  const muted = styles.getPropertyValue('--ui-foreground-muted').trim() || '#6b7280'
  const border = styles.getPropertyValue('--ui-border').trim() || (themeStore.isDark ? '#374151' : '#e5e7eb')
  const surface = styles.getPropertyValue('--ui-surface').trim() || (themeStore.isDark ? '#111827' : '#ffffff')
  const seriesColor = cssColor(props.color)

  const option: EChartsCoreOption = {
    animationDuration: 500,
    animationEasing: 'cubicOut',
    grid: { top: 44, right: 12, bottom: 28, left: 56 },
    tooltip: {
      trigger: 'axis',
      backgroundColor: surface,
      borderColor: border,
      textStyle: { color: foreground },
      valueFormatter: value => props.formatValue?.(Number(value)) ?? String(value),
    },
    xAxis: {
      type: 'time',
      boundaryGap: false,
      axisLine: { lineStyle: { color: border } },
      axisTick: { show: false },
      axisLabel: { color: muted, hideOverlap: true },
      splitLine: { show: false },
    },
    yAxis: {
      type: 'value',
      min: 0,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: {
        color: muted,
        formatter: (value: number) => props.formatValue?.(value) ?? String(value),
      },
      splitLine: { lineStyle: { color: border, opacity: 0.55, type: 'dashed' } },
    },
    series: [{
      type: 'line',
      data: props.values.map((value, index) => [sampleTime(index), value]),
      smooth: 0.28,
      showSymbol: false,
      symbol: 'circle',
      symbolSize: 7,
      lineStyle: { color: seriesColor, width: 2.5 },
      itemStyle: { color: seriesColor },
      areaStyle: { color: seriesColor, opacity: themeStore.isDark ? 0.12 : 0.16 },
      emphasis: { focus: 'series' },
    }],
    aria: {
      enabled: true,
      description: `Metric history with ${props.values.length} samples. Latest value: ${formattedLatest.value}.`,
    },
  }
  chart.setOption(option, { notMerge: true })
}

watch(
  [() => props.values, () => props.timestamps, () => props.color, () => themeStore.resolvedMode, () => themeStore.accent],
  () => { void nextTick(renderChart) },
  { deep: true },
)

onMounted(() => {
  renderChart()
  resizeObserver = new ResizeObserver(() => chart?.resize())
  if (chartElement.value) resizeObserver.observe(chartElement.value)
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  chart?.dispose()
  chart = null
})
</script>

<template>
  <div v-if="values.length" class="metric-chart">
    <div class="metric-value">{{ formattedLatest }}</div>
    <div ref="chartElement" class="metric-canvas" />
  </div>
  <div v-else class="metric-empty">{{ emptyText }}</div>
</template>

<style scoped>
.metric-chart { position: relative; min-height: 210px; }
.metric-canvas { width: 100%; height: 210px; }
.metric-value { position: absolute; z-index: 1; left: 12px; top: 2px; font-size: 20px; font-weight: 650; color: var(--ui-foreground); }
.metric-empty { height: 210px; display: grid; place-items: center; color: var(--ui-foreground-muted); }
</style>
