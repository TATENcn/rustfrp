<script setup lang="ts">
import { ref, onMounted, onUnmounted, h } from 'vue'
import {
  NCard,
  NButton,
  NTag,
  NSpace,
  NGrid,
  NGi,
  NStatistic,
  NInput,
  NSelect,
  NPopconfirm,
  NDataTable,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { useI18n } from '@/i18n'
import { formatDuration } from '@/i18n/format'
import { useSystemStore } from '@/stores/system'
import { downloadBackup, importFrpcToml } from '@/api/config'
import { extractApiError, resolveErrorMessage } from '@/api/errors'
import type { AvailableFrpVersion, FrpVersionList, InstalledFrpVersion } from '@/api/types'
import {
  activateFrpVersion,
  deleteFrpVersion,
  installFrpVersion,
  listAvailableFrpVersions,
  listFrpVersions,
} from '@/api/frp'
import AppIcon from '@/components/icon/AppIcon.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import LastUpdated from '@/components/common/LastUpdated.vue'
import RefreshButton from '@/components/common/RefreshButton.vue'
import StatusBadge from '@/components/common/StatusBadge.vue'
import DataState from '@/components/common/DataState.vue'
import ProcessTable from '@/components/processes/ProcessTable.vue'

const { t, locale } = useI18n()
const message = useMessage()
const systemStore = useSystemStore()

const reloadTaskId = ref<string | null>(null)
const reloadPolling = ref(false)
const reloadTaskStatus = ref<string | null>(null)
const configStatus = ref<'idle' | 'checking' | 'valid' | 'invalid'>('idle')
const importProfileName = ref('imported')
const importFile = ref<File | null>(null)
const importFileInput = ref<HTMLInputElement | null>(null)
const importing = ref(false)
const exporting = ref(false)
const frpVersions = ref<FrpVersionList>({ active: null, installed: [] })
const availableVersions = ref<AvailableFrpVersion[]>([])
const selectedVersion = ref<string | null>(null)
const mirrorMode = ref<'official' | 'custom'>('official')
const mirrorBase = ref('')
const frpBusy = ref(false)

const frpVersionColumns: DataTableColumns<InstalledFrpVersion> = [
  {
    title: t('frpVersion.version'), key: 'version', minWidth: 140,
    render: version => h(NSpace, { align: 'center', wrap: false }, {
      default: () => [version.version, version.active ? h(NTag, { type: 'success', size: 'small' }, { default: () => t('frpVersion.active') }) : null],
    }),
  },
  { title: t('frpVersion.platform'), key: 'platform', minWidth: 150 },
  {
    title: t('frpVersion.integrity'), key: 'integrity_ok', width: 130,
    render: version => h(NTag, { type: version.integrity_ok ? 'success' : 'error', size: 'small' }, { default: () => version.integrity_ok ? t('frpVersion.verified') : t('frpVersion.damaged') }),
  },
  {
    title: t('common.actions'), key: 'actions', width: 190,
    render: version => h(NSpace, null, {
      default: () => [
        h(NButton, { size: 'small', disabled: version.active || !version.integrity_ok || frpBusy.value, onClick: () => handleFrpActivate(version.version) }, { default: () => t('frpVersion.activate') }),
        h(NPopconfirm, { onPositiveClick: () => handleFrpDelete(version.version) }, {
          trigger: () => h(NButton, { size: 'small', type: 'error', disabled: version.active || frpBusy.value }, { default: () => t('common.delete') }),
          default: () => t('frpVersion.deleteConfirm', { version: version.version }),
        }),
      ],
    }),
  },
]

let reloadTimer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  systemStore.fetchStatus()
  loadFrpVersions()
  loadAvailableVersions()
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

function chooseImportFile() {
  importFileInput.value?.click()
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
      importFile.value = null
      if (importFileInput.value) importFileInput.value.value = ''
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
  <PageHeader :title="t('nav.status')" :description="locale === 'zh' ? '管理系统配置、FRP 版本并检查进程运行详情。' : 'Manage configuration, FRP versions, and inspect process details.'">
    <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="status" :size="21" /></span></template>
    <template #status><StatusBadge :status="systemStore.stale ? 'stale' : systemStore.status?.active_frpc_instances ? 'running' : 'stopped'" :label="systemStore.status?.state ?? 'unknown'" /></template>
    <template #actions><LastUpdated :value="systemStore.lastUpdated" :stale="systemStore.stale" /><RefreshButton :loading="systemStore.refreshing" @click="systemStore.fetchStatus" /></template>
  </PageHeader>

  <DataState :phase="systemStore.phase" :error="systemStore.error" @retry="systemStore.fetchStatus">
    <NSpace vertical :size="16">

      <NCard size="small" :bordered="false" class="shadow-card" title="System Actions">
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
        <div v-if="reloadTaskId" class="mt-2 text-xs text-foreground-muted">
          Task: {{ reloadTaskId }} · {{ reloadTaskStatus }}
        </div>
        <NTag v-if="configStatus !== 'idle'" :type="configStatus === 'valid' ? 'success' : 'error'" class="mt-2">
          {{ configStatus === 'valid' ? t('status.configValid') : configStatus === 'invalid' ? t('status.configInvalid') : 'Checking...' }}
        </NTag>
      </NCard>

      <NCard size="small" :bordered="false" class="shadow-card" :title="t('configTransfer.title')">
        <p class="m-0 text-sm text-foreground-muted">
          {{ t('configTransfer.description') }}
        </p>
        <div class="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.55fr)]">
          <section class="rounded-xl border border-border bg-surface-subtle p-4">
            <div class="font-medium">{{ t('configTransfer.importTitle') }}</div>
            <p class="mb-4 mt-1 text-xs text-foreground-muted">{{ t('configTransfer.importHint') }}</p>
            <div class="grid gap-3 sm:grid-cols-[minmax(180px,1fr)_minmax(0,1.4fr)]">
              <NInput v-model:value="importProfileName" :placeholder="t('configTransfer.profileName')" />
              <div class="flex min-w-0 items-center gap-2">
                <input ref="importFileInput" class="hidden" type="file" accept=".toml,text/plain" @change="selectImportFile" />
                <NButton class="shrink-0" @click="chooseImportFile">
                  <template #icon><AppIcon name="upload" /></template>{{ t('configTransfer.chooseFile') }}
                </NButton>
                <span class="min-w-0 truncate text-sm" :class="importFile ? 'text-foreground' : 'text-foreground-muted'">
                  {{ importFile?.name ?? t('configTransfer.noFile') }}
                </span>
              </div>
            </div>
            <div class="mt-4 flex justify-end">
              <NButton type="primary" :loading="importing" :disabled="!importFile || !importProfileName.trim()" @click="handleImport">
                <template #icon><AppIcon name="upload" /></template>{{ t('configTransfer.import') }}
              </NButton>
            </div>
          </section>
          <section class="flex flex-col rounded-xl border border-border bg-surface-subtle p-4">
            <div class="font-medium">{{ t('configTransfer.exportTitle') }}</div>
            <p class="mb-4 mt-1 text-xs text-foreground-muted">{{ t('configTransfer.exportHint') }}</p>
            <div class="mt-auto flex justify-end">
              <NButton :loading="exporting" @click="handleExport">
                <template #icon><AppIcon name="download" /></template>{{ t('configTransfer.export') }}
              </NButton>
            </div>
          </section>
        </div>
      </NCard>

      <NCard size="small" :bordered="false" class="shadow-card" :title="t('frpVersion.title')">
        <NSpace vertical>
          <p class="m-0 text-sm text-foreground-muted">
            {{ t('frpVersion.description') }}
          </p>
          <NSpace align="center">
            <NSelect
              v-model:value="selectedVersion"
              filterable
              tag
              :placeholder="t('frpVersion.select')"
              :options="availableVersions.map((release) => ({ label: release.version, value: release.version }))"
              class="w-44"
            />
            <NSelect
              v-model:value="mirrorMode"
              :options="[
                { label: t('frpVersion.official'), value: 'official' },
                { label: t('frpVersion.customMirror'), value: 'custom' },
              ]"
              class="w-48"
            />
            <NInput
              v-if="mirrorMode === 'custom'"
              v-model:value="mirrorBase"
              :placeholder="t('frpVersion.mirrorPlaceholder')"
              class="w-80"
            />
            <NButton type="primary" :loading="frpBusy" @click="handleFrpInstall">
              {{ t('frpVersion.install') }}
            </NButton>
          </NSpace>
          <NDataTable v-if="frpVersions.installed.length" :columns="frpVersionColumns" :data="frpVersions.installed" :row-key="row => row.version" :bordered="false" />
          <p v-else class="text-sm text-foreground-muted">{{ t('frpVersion.noneInstalled') }}</p>
        </NSpace>
      </NCard>

      <NGrid cols="1 s:2 m:4" :x-gap="12" :y-gap="12" v-if="systemStore.status">
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="State" :value="systemStore.status.state" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="Uptime" :value="formatUptime(systemStore.currentUptimeSecs ?? systemStore.status.uptime_secs)" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="Active frpc" :value="systemStore.status.active_frpc_instances" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="Total Profiles" :value="systemStore.status.total_profiles" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="Total Proxies" :value="systemStore.status.total_proxies" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="Total Bindings" :value="systemStore.status.total_bindings" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small" :bordered="false" class="shadow-card">
            <NStatistic label="Total Visitors" :value="systemStore.status.total_visitors" />
          </NCard>
        </NGi>
      </NGrid>

      <NCard title="FRP Processes" size="small" :bordered="false" class="shadow-card">
        <ProcessTable v-if="systemStore.status?.processes?.length" :processes="systemStore.status.processes" />
        <p v-else class="py-8 text-center text-sm text-foreground-muted">No frpc processes running</p>
      </NCard>
    </NSpace>
  </DataState>
</template>
