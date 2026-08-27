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
  NSelect,
  NPopconfirm,
  useMessage,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { formatDuration } from '@/i18n/format'
import { useSystemStore } from '@/stores/system'
import { downloadBackup, importFrpcToml } from '@/api/config'
import { extractApiError, resolveErrorMessage } from '@/api/errors'
import type { AvailableFrpVersion, FrpVersionList, StatusResponse } from '@/api/types'
import {
  activateFrpVersion,
  deleteFrpVersion,
  installFrpVersion,
  listAvailableFrpVersions,
  listFrpVersions,
} from '@/api/frp'

const { t, locale } = useI18n()
const message = useMessage()
const systemStore = useSystemStore()

const reloadTaskId = ref<string | null>(null)
const reloadPolling = ref(false)
const reloadTaskStatus = ref<string | null>(null)
const configStatus = ref<'idle' | 'checking' | 'valid' | 'invalid'>('idle')
const importProfileName = ref('imported')
const importFile = ref<File | null>(null)
const importing = ref(false)
const exporting = ref(false)
const frpVersions = ref<FrpVersionList>({ active: null, installed: [] })
const availableVersions = ref<AvailableFrpVersion[]>([])
const selectedVersion = ref<string | null>(null)
const mirrorMode = ref<'official' | 'custom'>('official')
const mirrorBase = ref('')
const frpBusy = ref(false)

let timer: ReturnType<typeof setInterval> | null = null
let reloadTimer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  systemStore.fetchStatus()
  loadFrpVersions()
  loadAvailableVersions()
  timer = setInterval(() => systemStore.fetchStatus(), 10_000)
})

async function loadFrpVersions() {
  try {
    frpVersions.value = (await listFrpVersions()).data ?? { active: null, installed: [] }
  } catch (error) {
    message.error(extractApiError(error).message)
  }
}

async function loadAvailableVersions() {
  try {
    availableVersions.value = (await listAvailableFrpVersions()).data ?? []
    selectedVersion.value ??= availableVersions.value[0]?.version ?? null
  } catch {
    // Installed version management remains available while GitHub is unreachable.
  }
}

async function handleFrpInstall() {
  if (!selectedVersion.value) {
    message.warning(t('frpVersion.selectRequired'))
    return
  }
  if (mirrorMode.value === 'custom' && !mirrorBase.value.startsWith('https://')) {
    message.warning(t('frpVersion.httpsRequired'))
    return
  }
  frpBusy.value = true
  try {
    await installFrpVersion(
      selectedVersion.value,
      mirrorMode.value === 'custom' ? mirrorBase.value.trim() : undefined,
    )
    message.success(t('frpVersion.installSuccess', { version: selectedVersion.value }))
    await loadFrpVersions()
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    frpBusy.value = false
  }
}

async function handleFrpActivate(version: string) {
  frpBusy.value = true
  try {
    await activateFrpVersion(version)
    message.success(t('frpVersion.activateSuccess', { version }))
    await Promise.all([loadFrpVersions(), systemStore.fetchStatus()])
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    frpBusy.value = false
  }
}

async function handleFrpDelete(version: string) {
  frpBusy.value = true
  try {
    await deleteFrpVersion(version)
    message.success(t('frpVersion.deleteSuccess', { version }))
    await loadFrpVersions()
  } catch (error) {
    message.error(extractApiError(error).message)
  } finally {
    frpBusy.value = false
  }
}

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

const formatUptime = (seconds: number) => formatDuration(seconds, locale.value)

function selectImportFile(event: Event) {
  importFile.value = (event.target as HTMLInputElement).files?.[0] ?? null
}

async function handleImport() {
  if (!importFile.value || !importProfileName.value.trim()) {
    message.warning(t('configTransfer.selectFile'))
    return
  }
  importing.value = true
  try {
    const result = await importFrpcToml(importProfileName.value.trim(), await importFile.value.text())
    const summary = result.data
    if (summary) {
      message.success(t('configTransfer.importSuccess', {
        profile: summary.profile_name,
        proxies: summary.proxies_imported,
        visitors: summary.visitors_imported,
      }))
      await systemStore.fetchStatus()
    }
  } catch (error) {
    message.error(t(resolveErrorMessage(extractApiError(error).code)))
  } finally {
    importing.value = false
  }
}

async function handleExport() {
  exporting.value = true
  try {
    await downloadBackup()
    message.success(t('configTransfer.exportSuccess'))
  } catch {
    message.error(t('error.serverError'))
  } finally {
    exporting.value = false
  }
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

      <NCard size="small" :title="t('configTransfer.title')">
        <NSpace vertical>
          <p style="margin: 0; color: var(--n-text-color-2)">
            {{ t('configTransfer.description') }}
          </p>
          <NSpace align="center">
            <NInput
              v-model:value="importProfileName"
              :placeholder="t('configTransfer.profileName')"
              style="width: 220px"
            />
            <input type="file" accept=".toml,text/plain" @change="selectImportFile" />
            <NButton type="primary" :loading="importing" @click="handleImport">
              {{ t('configTransfer.import') }}
            </NButton>
            <NButton :loading="exporting" @click="handleExport">
              {{ t('configTransfer.export') }}
            </NButton>
          </NSpace>
        </NSpace>
      </NCard>

      <NCard size="small" :title="t('frpVersion.title')">
        <NSpace vertical>
          <p style="margin: 0; color: var(--n-text-color-2)">
            {{ t('frpVersion.description') }}
          </p>
          <NSpace align="center">
            <NSelect
              v-model:value="selectedVersion"
              filterable
              tag
              :placeholder="t('frpVersion.select')"
              :options="availableVersions.map((release) => ({ label: release.version, value: release.version }))"
              style="width: 180px"
            />
            <NSelect
              v-model:value="mirrorMode"
              :options="[
                { label: t('frpVersion.official'), value: 'official' },
                { label: t('frpVersion.customMirror'), value: 'custom' },
              ]"
              style="width: 190px"
            />
            <NInput
              v-if="mirrorMode === 'custom'"
              v-model:value="mirrorBase"
              :placeholder="t('frpVersion.mirrorPlaceholder')"
              style="width: 360px"
            />
            <NButton type="primary" :loading="frpBusy" @click="handleFrpInstall">
              {{ t('frpVersion.install') }}
            </NButton>
          </NSpace>
          <table v-if="frpVersions.installed.length" style="width: 100%; border-collapse: collapse">
            <thead>
              <tr style="text-align: left; border-bottom: 1px solid var(--n-border-color)">
                <th style="padding: 8px">{{ t('frpVersion.version') }}</th>
                <th style="padding: 8px">{{ t('frpVersion.platform') }}</th>
                <th style="padding: 8px">{{ t('frpVersion.integrity') }}</th>
                <th style="padding: 8px">{{ t('common.actions') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="version in frpVersions.installed" :key="version.version" style="border-bottom: 1px solid var(--n-border-color)">
                <td style="padding: 8px">
                  {{ version.version }}
                  <NTag v-if="version.active" type="success" size="small">{{ t('frpVersion.active') }}</NTag>
                </td>
                <td style="padding: 8px">{{ version.platform }}</td>
                <td style="padding: 8px">
                  <NTag :type="version.integrity_ok ? 'success' : 'error'" size="small">
                    {{ version.integrity_ok ? t('frpVersion.verified') : t('frpVersion.damaged') }}
                  </NTag>
                </td>
                <td style="padding: 8px">
                  <NSpace>
                    <NButton size="small" :disabled="version.active || !version.integrity_ok || frpBusy" @click="handleFrpActivate(version.version)">
                      {{ t('frpVersion.activate') }}
                    </NButton>
                    <NPopconfirm @positive-click="handleFrpDelete(version.version)">
                      <template #trigger>
                        <NButton size="small" type="error" :disabled="version.active || frpBusy">
                          {{ t('common.delete') }}
                        </NButton>
                      </template>
                      {{ t('frpVersion.deleteConfirm', { version: version.version }) }}
                    </NPopconfirm>
                  </NSpace>
                </td>
              </tr>
            </tbody>
          </table>
          <p v-else style="color: var(--n-text-color-3)">{{ t('frpVersion.noneInstalled') }}</p>
        </NSpace>
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
