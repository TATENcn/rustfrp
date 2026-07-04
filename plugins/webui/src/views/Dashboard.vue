<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { NCard, NGrid, NGi, NSpace, NStatistic, NSpin, NTag } from 'naive-ui'
import { useI18n } from '@/i18n'
import { useSystemStore } from '@/stores/system'

const { t } = useI18n()
const systemStore = useSystemStore()

let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  systemStore.fetchStatus()
  timer = setInterval(() => systemStore.fetchStatus(), 10_000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  return `${h}h ${m}m ${s}s`
}
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
