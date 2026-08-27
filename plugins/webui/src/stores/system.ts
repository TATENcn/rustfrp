import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import type { StatusResponse } from '@/api/types'
import * as systemApi from '@/api/system'
import { extractApiError } from '@/api/errors'

export const useSystemStore = defineStore('system', () => {
  const status = ref<StatusResponse | null>(null)
  const loading = ref(false)
  const refreshing = ref(false)
  const error = ref<string | null>(null)
  const lastUpdated = ref<number | null>(null)
  const failureCount = ref(0)
  const stale = ref(false)
  let inFlight: Promise<void> | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let polling = false

  const phase = computed(() => {
    if (loading.value && !status.value) return 'loading'
    if (error.value && !status.value) return 'error'
    if (stale.value) return 'stale'
    if (refreshing.value) return 'refreshing'
    if (status.value) return 'ready'
    return 'idle'
  })

  function requestStatus(): Promise<void> {
    if (inFlight) return inFlight
    const hasData = status.value !== null
    loading.value = !hasData
    refreshing.value = hasData
    error.value = null
    inFlight = systemApi.getStatus()
      .then((resp) => {
        status.value = resp.data ?? null
        lastUpdated.value = Date.now()
        failureCount.value = 0
        stale.value = false
      })
      .catch((e) => {
        error.value = extractApiError(e).message
        failureCount.value += 1
        stale.value = status.value !== null
      })
      .finally(() => {
        loading.value = false
        refreshing.value = false
        inFlight = null
      })
    return inFlight
  }

  async function fetchStatus() { await requestStatus() }

  function nextDelay() {
    if (document.hidden) return 60_000
    if (failureCount.value === 0) return 10_000
    return Math.min(60_000, 2 ** Math.min(failureCount.value - 1, 6) * 2_000)
  }

  async function tick() {
    await requestStatus()
    if (polling) timer = setTimeout(tick, nextDelay())
  }

  function startPolling() {
    if (polling) return
    polling = true
    void tick()
  }

  function stopPolling() {
    polling = false
    if (timer) clearTimeout(timer)
    timer = null
  }

  function handleVisibility() {
    if (!polling) return
    if (timer) clearTimeout(timer)
    timer = null
    if (!document.hidden && (!lastUpdated.value || Date.now() - lastUpdated.value > 10_000)) void tick()
    else timer = setTimeout(tick, nextDelay())
  }

  document.addEventListener('visibilitychange', handleVisibility)

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

  return {
    status, loading, refreshing, error, lastUpdated, failureCount, stale, phase,
    fetchStatus, startPolling, stopPolling, triggerReload, getReloadStatus,
  }
})
