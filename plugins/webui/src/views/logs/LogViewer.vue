<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { NAlert, NCard, NTabPane, NTabs, useMessage } from 'naive-ui'
import { useI18n } from '@/i18n'
import { getLogs, type LogResponse } from '@/api/logs'
import { listProfiles } from '@/api/profiles'
import type { FrpsProfile } from '@/api/types'
import AppIcon from '@/components/icon/AppIcon.vue'
import PageHeader from '@/components/common/PageHeader.vue'
import StatusBadge from '@/components/common/StatusBadge.vue'
import LastUpdated from '@/components/common/LastUpdated.vue'
import LogToolbar from '@/components/logs/LogToolbar.vue'
import LogConsole from '@/components/logs/LogConsole.vue'

const { t, locale } = useI18n()
const message = useMessage()
const profiles = ref<FrpsProfile[]>([])
const selectedProfile = ref<number | null>(null)
const logType = ref<'combined' | 'stdout' | 'stderr'>('combined')
const lines = ref(200)
const search = ref('')
const loading = ref(false)
const error = ref<string | null>(null)
const log = ref<LogResponse | null>(null)
const lastUpdated = ref<number | null>(null)
const profileOptions = computed(() => profiles.value.map(profile => ({ label: profile.name, value: profile.id ?? -1 })))

async function loadProfiles() {
  try {
    profiles.value = (await listProfiles()).data ?? []
    selectedProfile.value ??= profiles.value[0]?.id ?? null
  } catch (reason) { error.value = String(reason) }
}
async function fetchLogs() {
  if (selectedProfile.value === null) { error.value = t('logs.selectProfile'); return }
  loading.value = true
  error.value = null
  try {
    log.value = (await getLogs(selectedProfile.value, { lines: lines.value, log_type: logType.value })).data ?? null
    lastUpdated.value = Date.now()
  } catch (reason) { error.value = String(reason) }
  finally { loading.value = false }
}
function clear() { log.value = null }
async function copy() {
  if (!log.value?.content) return
  try { await navigator.clipboard.writeText(log.value.content); message.success(locale.value === 'zh' ? '日志已复制' : 'Logs copied') }
  catch { message.error(locale.value === 'zh' ? '复制失败' : 'Copy failed') }
}
function download() {
  if (!log.value?.content) return
  const url = URL.createObjectURL(new Blob([log.value.content], { type: 'text/plain;charset=utf-8' }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = `${log.value.profile_name}-${log.value.log_type}.log`
  anchor.click()
  URL.revokeObjectURL(url)
}
watch([selectedProfile, logType], () => { if (selectedProfile.value !== null) void fetchLogs() })
onMounted(async () => { await loadProfiles(); if (selectedProfile.value !== null) await fetchLogs() })
</script>

<template>
  <PageHeader :title="t('nav.logs')" :description="locale === 'zh' ? '查看和筛选各 frpc 实例的标准输出与错误日志。' : 'Inspect and filter stdout and stderr from each frpc instance.'">
    <template #icon><span class="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary"><AppIcon name="logs" :size="21" /></span></template>
    <template #status><StatusBadge :status="error ? 'error' : loading ? 'refreshing' : 'idle'" :label="error ? (locale === 'zh' ? '读取失败' : 'Read failed') : loading ? (locale === 'zh' ? '加载中' : 'Loading') : (locale === 'zh' ? '按需读取' : 'On demand')" /></template>
    <template #actions><LastUpdated :value="lastUpdated" /></template>
  </PageHeader>

  <NAlert v-if="error" type="error" closable class="mb-4" @close="error = null">{{ error }}</NAlert>
  <NCard :bordered="false" content-style="padding: 0" class="overflow-hidden shadow-card">
    <LogToolbar v-model:profile="selectedProfile" v-model:lines="lines" v-model:search="search" :profile-options="profileOptions" :loading="loading" @refresh="fetchLogs" @clear="clear" @copy="copy" @download="download" />
    <NTabs v-model:value="logType" type="line" animated class="px-4 pt-2">
      <NTabPane name="combined" :tab="t('logs.combined')" />
      <NTabPane name="stdout" :tab="t('logs.stdout')" />
      <NTabPane name="stderr" :tab="t('logs.stderr')" />
    </NTabs>
    <LogConsole :content="log?.content ?? ''" :search="search" :stream="logType" />
  </NCard>
</template>
