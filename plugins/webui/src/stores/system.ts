import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { StatusResponse } from '@/api/types'
import * as systemApi from '@/api/system'
import { extractApiError } from '@/api/errors'

export const useSystemStore = defineStore('system', () => {
  const status = ref<StatusResponse | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetchStatus() {
    loading.value = true
    error.value = null
    try {
      const resp = await systemApi.getStatus()
      status.value = resp.data ?? null
    } catch (e) {
      error.value = extractApiError(e).message
    } finally {
      loading.value = false
    }
  }

  async function triggerReload(): Promise<string | null> {
    try {
      const resp = await systemApi.triggerReload()
      return resp.data?.task_id ?? null
    } catch (e) {
      error.value = extractApiError(e).message
      throw e
    }
  }

  async function getReloadStatus(taskId: string) {
    try {
      const resp = await systemApi.getReloadStatus(taskId)
      return resp.data ?? null
    } catch (e) {
      error.value = extractApiError(e).message
      throw e
    }
  }

  return { status, loading, error, fetchStatus, triggerReload, getReloadStatus }
})
