<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import {
  NCard,
  NButton,
  NTag,
  NSpace,
  NGrid,
  NGi,
  NStatistic,
  NSpin,
  NDivider,
  NInput,
  useMessage,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { useSystemStore } from '@/stores/system'
import type { StatusResponse } from '@/api/types'

const { t } = useI18n()
const message = useMessage()
const systemStore = useSystemStore()

const reloadTaskId = ref<string | null>(null)
const reloadPolling = ref(false)
const reloadTaskStatus = ref<string | null>(null)
const configStatus = ref<'idle' | 'checking' | 'valid' | 'invalid'>('idle')

let timer: ReturnType<typeof setInterval> | null = null
let reloadTimer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  systemStore.fetchStatus()
  timer = setInterval(() => systemStore.fetchStatus(), 10_000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
  if (reloadTimer) clearInterval(reloadTimer)
})

async function handleReload() {
  try {
    const taskId = await systemStore.triggerReload()
    if (taskId) {
      reloadTaskId.value = taskId
      reloadPolling.value = true
      reloadTaskStatus.value = 'Reloading...'

      reloadTimer = setInterval(async () => {
        try {
          const status = await systemStore.getReloadStatus(taskId)
          if (status) {
            reloadTaskStatus.value = `Status: ${status.status} · ${status.profiles_affected} profiles`
            if (status.status === 'completed' || status.status === 'failed') {
              reloadPolling.value = false
              if (reloadTimer) clearInterval(reloadTimer)
              if (status.errors.length) {
                message.error(status.errors.join(', '))
              } else {
                message.success('Reload completed')
              }
            }
          }
        } catch {
          reloadPolling.value = false
          if (reloadTimer) clearInterval(reloadTimer)
        }
      }, 2000)
      message.info(`Reload started: ${taskId}`)
    }
  } catch {
    message.error(t('error.serverError'))
  }
}

async function checkConfig() {
  configStatus.value = 'checking'
  try {
    await systemStore.fetchStatus()
    configStatus.value = 'valid'
    message.success(t('status.configValid'))
  } catch {
    configStatus.value = 'invalid'
    message.error(t('status.configInvalid'))
  }
}

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  return `${h}h ${m}m ${s}s`
}
</script>

<template>
  <NSpin :show="systemStore.loading && !systemStore.status">
    <NSpace vertical>
      <h3 style="margin: 0">{{ t('nav.status') }}</h3>

      <NCard size="small" title="System Actions">
        <NSpace>
          <NButton type="primary" @click="handleReload" :loading="reloadPolling">
            {{ t('app.reload') }}
          </NButton>
          <NButton @click="checkConfig" :loading="configStatus === 'checking'">
            Check Config
          </NButton>
          <NButton @click="systemStore.fetchStatus()">
            Refresh Status
          </NButton>
        </NSpace>
        <div v-if="reloadTaskId" style="margin-top: 8px; font-size: 13px; color: var(--n-text-color-2)">
          Task: {{ reloadTaskId }} · {{ reloadTaskStatus }}
        </div>
        <NTag v-if="configStatus !== 'idle'" :type="configStatus === 'valid' ? 'success' : 'error'" style="margin-top: 8px">
          {{ configStatus === 'valid' ? t('status.configValid') : configStatus === 'invalid' ? t('status.configInvalid') : 'Checking...' }}
        </NTag>
      </NCard>

      <NGrid cols="1 s:2 m:4" :x-gap="12" :y-gap="12" v-if="systemStore.status">
        <NGi>
          <NCard size="small">
            <NStatistic label="State" :value="systemStore.status.state" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="Uptime" :value="formatUptime(systemStore.status.uptime_secs)" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="Active frpc" :value="systemStore.status.active_frpc_instances" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="Total Profiles" :value="systemStore.status.total_profiles" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="Total Proxies" :value="systemStore.status.total_proxies" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="Total Bindings" :value="systemStore.status.total_bindings" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="Total Visitors" :value="systemStore.status.total_visitors" />
          </NCard>
        </NGi>
      </NGrid>

      <NDivider />

      <!-- Processes -->
      <NCard title="FRP Processes" size="small" v-if="systemStore.status?.processes?.length">
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
      <NCard v-else size="small">
        <p style="color: var(--n-text-color-3)">No frpc processes running</p>
      </NCard>
    </NSpace>
  </NSpin>
</template>
