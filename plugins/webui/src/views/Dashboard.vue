<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { NCard, NGrid, NGi, NSpace, NStatistic, NSpin, NTag } from 'naive-ui'
import { useI18n } from '@/i18n'
import { formatBytes, formatDuration, formatPercent } from '@/i18n/format'
import { useSystemStore } from '@/stores/system'
import { useEnvironmentStore } from '@/stores/environments'
import { getMetricsHistory } from '@/api/metrics'
import type { MetricsHistory } from '@/api/types'
import MetricChart from '@/components/MetricChart.vue'

const { t, locale } = useI18n()
const systemStore = useSystemStore()
const environmentStore = useEnvironmentStore()
const history = ref<MetricsHistory>({ resources: [], traffic: [] })

const cpuValues = computed(() => history.value.resources.map((sample) => sample.system_cpu_percent))
const memoryValues = computed(() => history.value.resources.map((sample) => sample.system_memory_used_bytes))
const receivedValues = computed(() => history.value.traffic.map((sample) => sample.received_bytes))
const sentValues = computed(() => history.value.traffic.map((sample) => sample.sent_bytes))

async function fetchHistory() {
  const response = await getMetricsHistory(environmentStore.activeId ?? undefined)
  history.value = response.data ?? { resources: [], traffic: [] }
}

let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  systemStore.fetchStatus()
  environmentStore.fetchAll().then(fetchHistory)
  timer = setInterval(() => { systemStore.fetchStatus(); fetchHistory() }, 10_000)
})

watch(() => environmentStore.activeId, () => fetchHistory())

onUnmounted(() => {
  if (timer) clearInterval(timer)
})

const formatUptime = (seconds: number) => formatDuration(seconds, locale.value)
const formatCpu = (value: number) => formatPercent(value, locale.value)
const formatMemory = (value: number) => formatBytes(value, locale.value)
</script>

<template>
  <NSpin :show="systemStore.loading && !systemStore.status">
    <NGrid cols="1 s:2 m:3 l:4" :x-gap="16" :y-gap="16">
      <!-- State -->
      <NGi>
        <NCard size="small">
          <NStatistic label="State">
            <NTag :type="systemStore.status?.active_frpc_instances ? 'success' : 'default'">
              {{ systemStore.status?.state ?? '-' }}
            </NTag>
          </NStatistic>
        </NCard>
      </NGi>

      <!-- Uptime -->
      <NGi>
        <NCard size="small">
          <NStatistic :label="t('status.uptime', { duration: '' })" :value="systemStore.status ? formatUptime(systemStore.status.uptime_secs) : '-'" />
        </NCard>
      </NGi>

      <!-- Active frpc -->
      <NGi>
        <NCard size="small">
          <NStatistic :label="t('status.frpcRunning', { count: 0 })" :value="systemStore.status?.active_frpc_instances ?? 0" />
        </NCard>
      </NGi>

      <!-- Total Profiles -->
      <NGi>
        <NCard size="small">
          <NStatistic label="Profiles" :value="systemStore.status?.total_profiles ?? 0" />
        </NCard>
      </NGi>

      <!-- Total Proxies -->
      <NGi>
        <NCard size="small">
          <NStatistic label="Proxies" :value="systemStore.status?.total_proxies ?? 0" />
        </NCard>
      </NGi>

      <!-- Total Bindings -->
      <NGi>
        <NCard size="small">
          <NStatistic label="Bindings" :value="systemStore.status?.total_bindings ?? 0" />
        </NCard>
      </NGi>

      <!-- Total Visitors -->
      <NGi>
        <NCard size="small">
          <NStatistic label="Visitors" :value="systemStore.status?.total_visitors ?? 0" />
        </NCard>
      </NGi>
    </NGrid>

    <NGrid cols="1 m:2" :x-gap="16" :y-gap="16" style="margin-top: 16px">
      <NGi><NCard title="System CPU" size="small"><MetricChart :values="cpuValues" :format-value="formatCpu" /></NCard></NGi>
      <NGi><NCard title="System memory" size="small"><MetricChart :values="memoryValues" color="#2080f0" :format-value="formatMemory" /></NCard></NGi>
      <NGi><NCard title="Traffic received" size="small"><MetricChart :values="receivedValues" color="#18a058" :format-value="formatMemory" empty-text="No traffic samples yet" /></NCard></NGi>
      <NGi><NCard title="Traffic sent" size="small"><MetricChart :values="sentValues" color="#f0a020" :format-value="formatMemory" empty-text="No traffic samples yet" /></NCard></NGi>
    </NGrid>

    <!-- Process List -->
    <NCard
      v-if="systemStore.status?.processes?.length"
      title="FRP Processes"
      size="small"
      style="margin-top: 16px"
    >
      <table style="width: 100%; border-collapse: collapse">
        <thead>
          <tr style="text-align: left; border-bottom: 1px solid var(--n-border-color)">
            <th style="padding: 8px">Profile</th>
            <th style="padding: 8px">PID</th>
            <th style="padding: 8px">Status</th>
            <th style="padding: 8px">Restarts</th>
            <th style="padding: 8px">Config</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="p in systemStore.status.processes"
            :key="p.profile_id"
            style="border-bottom: 1px solid var(--n-border-color)"
          >
            <td style="padding: 8px">{{ p.profile_name }}</td>
            <td style="padding: 8px">{{ p.pid ?? '-' }}</td>
            <td style="padding: 8px">
              <NTag :type="p.running ? 'success' : 'error'" size="small">
                {{ p.running ? 'Running' : 'Stopped' }}
              </NTag>
            </td>
            <td style="padding: 8px">{{ p.restart_count }}</td>
            <td style="padding: 8px; font-size: 12px; color: var(--n-text-color-3)">
              {{ p.config_path }}
            </td>
          </tr>
        </tbody>
      </table>
    </NCard>
  </NSpin>
</template>
