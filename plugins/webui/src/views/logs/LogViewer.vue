<script setup lang="ts">
import { useI18n } from '@/i18n'

import {
  NSelect,
  NRadioGroup,
  NRadioButton,
  NInputNumber,
  NButton,
  NCard,
  NSpace,
  NEmpty,
  NSpin,
} from 'naive-ui'
import { getLogs, type LogResponse } from '@/api/logs'
import { listProfiles } from '@/api/profiles'
import type { FrpsProfile } from '@/api/types'
import ErrorAlert from '@/components/ErrorAlert.vue'

const { t } = useI18n()

const profiles = ref<FrpsProfile[]>([])
const selectedProfile = ref<number | null>(null)
const logType = ref<'combined' | 'stdout' | 'stderr'>('combined')
const lines = ref(200)
const loading = ref(false)
const error = ref<string | null>(null)
const log = ref<LogResponse | null>(null)

const profileOptions = computed(() =>
  profiles.value.map((p) => ({ label: p.name, value: p.id ?? -1 })),
)

async function loadProfiles() {
  try {
    const resp = await listProfiles()
    profiles.value = resp.data ?? []
    if (profiles.value.length > 0 && selectedProfile.value === null) {
      selectedProfile.value = profiles.value[0].id ?? null
    }
  } catch (e) {
    error.value = String(e)
  }
}

async function fetchLogs() {
  if (selectedProfile.value === null) {
    error.value = t('logs.selectProfile')
    return
  }
  loading.value = true
  error.value = null
  try {
    const resp = await getLogs(selectedProfile.value, {
      lines: lines.value,
      log_type: logType.value,
    })
    log.value = resp.data ?? null
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

onMounted(loadProfiles)
</script>

<template>
  <div>
    <ErrorAlert :error="error" @dismiss="error = null" />

    <NSpace align="center" style="margin-bottom: 16px">
      <NSelect
        v-model:value="selectedProfile"
        :options="profileOptions"
        :placeholder="t('logs.profile')"
        style="width: 240px"
      />
      <NRadioGroup v-model:value="logType">
        <NRadioButton value="combined">{{ t('logs.combined') }}</NRadioButton>
        <NRadioButton value="stdout">{{ t('logs.stdout') }}</NRadioButton>
        <NRadioButton value="stderr">{{ t('logs.stderr') }}</NRadioButton>
      </NRadioGroup>
      <NInputNumber
        v-model:value="lines"
        :min="10"
        :max="1000"
        style="width: 120px"
      />
      <NButton type="primary" :loading="loading" @click="fetchLogs">
        {{ t('common.refresh') }}
      </NButton>
    </NSpace>

    <NCard :title="log ? `${log.profile_name} · ${log.log_type}` : t('logs.title')">
      <NSpin :show="loading">
        <pre
          v-if="log && log.content"
          style="
            white-space: pre-wrap;
            word-break: break-all;
            max-height: 60vh;
            overflow: auto;
            font-family: monospace;
            font-size: 12px;
            margin: 0;
          "
          >{{ log.content }}</pre
        >
        <NEmpty v-else :description="t('logs.empty')" />
      </NSpin>
    </NCard>
  </div>
</template>
