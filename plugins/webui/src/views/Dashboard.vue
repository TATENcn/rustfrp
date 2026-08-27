<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue'
import { NCard } from 'naive-ui'
import { useI18n } from '@/i18n'
import { formatBytes, formatDuration, formatPercent } from '@/i18n/format'
import { useSystemStore } from '@/stores/system'
import { useMetricsStore } from '@/stores/metrics'
import { useEnvironmentStore } from '@/stores/environments'
import AppIcon from '@/components/icon/AppIcon.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import DataState from '@/components/common/DataState.vue'
import LastUpdated from '@/components/common/LastUpdated.vue'
import RefreshButton from '@/components/common/RefreshButton.vue'
import StatusBadge from '@/components/common/StatusBadge.vue'
import MetricCard from '@/components/metrics/MetricCard.vue'
import ProcessTable from '@/components/processes/ProcessTable.vue'
import MetricChart from '@/components/MetricChart.vue'

const { t, locale } = useI18n()
const systemStore = useSystemStore()
const metricsStore = useMetricsStore()
const environmentStore = useEnvironmentStore()
const isZh = computed(() => locale.value === 'zh')
const history = computed(() => metricsStore.history)
const latestResource = computed(() => history.value.resources.at(-1))
const cpuValues = computed(() => history.value.resources.map(sample => sample.system_cpu_percent))
const memoryValues = computed(() => history.value.resources.map(sample => sample.system_memory_used_bytes))
const receivedValues = computed(() => history.value.traffic.map(sample => sample.received_bytes))
const sentValues = computed(() => history.value.traffic.map(sample => sample.sent_bytes))
const memoryPercentage = computed(() => latestResource.value?.system_memory_total_bytes ? Math.round(latestResource.value.system_memory_used_bytes / latestResource.value.system_memory_total_bytes * 100) : 0)
const refresh = () => Promise.all([systemStore.fetchStatus(), metricsStore.refresh()])
watch(() => environmentStore.activeId, id => { void metricsStore.selectEnvironment(id) })
onMounted(() => { void metricsStore.selectEnvironment(environmentStore.activeId); metricsStore.startPolling() })
onUnmounted(() => metricsStore.stopPolling())
</script>

<template>
  <PageHeader :title="t('nav.dashboard')" :description="isZh ? '系统运行状态、资源使用与流量趋势概览。' : 'System health, resource usage, and traffic trends.'">
    <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="dashboard" :size="21" /></span></template>
    <template #status><StatusBadge :status="systemStore.error ? 'stale' : systemStore.status?.active_frpc_instances ? 'running' : 'stopped'" :label="systemStore.status?.state ?? 'unknown'" /></template>
    <template #actions><LastUpdated :value="systemStore.lastUpdated" :stale="systemStore.stale" /><RefreshButton :loading="systemStore.refreshing || metricsStore.refreshing" @click="refresh" /></template>
  </PageHeader>

  <DataState :phase="systemStore.phase" :error="systemStore.error" @retry="refresh">
    <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <MetricCard :label="isZh ? '运行时间' : 'Uptime'" :value="systemStore.status ? formatDuration(systemStore.status.uptime_secs, locale) : '—'" icon="clock" />
      <MetricCard :label="isZh ? '运行中的 frpc' : 'Active frpc'" :value="systemStore.status?.active_frpc_instances ?? 0" icon="running" :detail="`${systemStore.status?.total_profiles ?? 0} ${isZh ? '个配置' : 'profiles'}`" />
      <MetricCard :label="isZh ? '系统 CPU' : 'System CPU'" :value="latestResource ? formatPercent(latestResource.system_cpu_percent, locale) : '—'" icon="cpu" :percentage="latestResource?.system_cpu_percent ?? 0" />
      <MetricCard :label="isZh ? '系统内存' : 'System memory'" :value="latestResource ? formatBytes(latestResource.system_memory_used_bytes, locale) : '—'" icon="memory" :percentage="memoryPercentage" />
    </div>

    <div class="mt-4 grid gap-4 lg:grid-cols-2">
      <NCard size="small" :bordered="false" class="shadow-card"><template #header><div class="flex items-center gap-2"><AppIcon name="cpu" class="text-primary" />{{ isZh ? 'CPU 历史' : 'CPU history' }}</div></template><MetricChart :values="cpuValues" :format-value="value => formatPercent(value, locale)" /></NCard>
      <NCard size="small" :bordered="false" class="shadow-card"><template #header><div class="flex items-center gap-2"><AppIcon name="memory" class="text-primary" />{{ isZh ? '内存历史' : 'Memory history' }}</div></template><MetricChart :values="memoryValues" color="var(--ui-primary)" :format-value="value => formatBytes(value, locale)" /></NCard>
      <NCard size="small" :bordered="false" class="shadow-card"><template #header><div class="flex items-center gap-2"><AppIcon name="arrow-down" class="text-success" />{{ isZh ? '接收流量' : 'Traffic received' }}</div></template><MetricChart :values="receivedValues" color="var(--ui-success)" :format-value="value => formatBytes(value, locale)" :empty-text="isZh ? '暂无流量样本' : 'No traffic samples yet'" /></NCard>
      <NCard size="small" :bordered="false" class="shadow-card"><template #header><div class="flex items-center gap-2"><AppIcon name="arrow-up" class="text-warning" />{{ isZh ? '发送流量' : 'Traffic sent' }}</div></template><MetricChart :values="sentValues" color="var(--ui-warning)" :format-value="value => formatBytes(value, locale)" :empty-text="isZh ? '暂无流量样本' : 'No traffic samples yet'" /></NCard>
    </div>

    <NCard class="mt-4 shadow-card" :bordered="false" :title="isZh ? 'FRP 进程' : 'FRP processes'">
      <ProcessTable v-if="systemStore.status?.processes.length" :processes="systemStore.status.processes" />
      <div v-else class="py-10 text-center text-sm text-foreground-muted">{{ isZh ? '当前没有 frpc 进程' : 'No frpc processes' }}</div>
    </NCard>
  </DataState>
</template>
