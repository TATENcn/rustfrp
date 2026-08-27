import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { getMetricsHistory } from '@/api/metrics'
import { extractApiError } from '@/api/errors'
import type { MetricsHistory } from '@/api/types'

const emptyHistory = (): MetricsHistory => ({ resources: [], traffic: [] })

export const useMetricsStore = defineStore('metrics', () => {
  const history = ref<MetricsHistory>(emptyHistory())
  const loading = ref(false)
  const refreshing = ref(false)
  const error = ref<string | null>(null)
  const lastUpdated = ref<number | null>(null)
  const failureCount = ref(0)
  const environmentId = ref<number | null>(null)
  let inFlight: Promise<void> | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let polling = false

  const hasData = computed(() => history.value.resources.length > 0 || history.value.traffic.length > 0)
  const phase = computed(() => loading.value && !hasData.value ? 'loading' : error.value && !hasData.value ? 'error' : error.value ? 'stale' : refreshing.value ? 'refreshing' : 'ready')

  function refresh(): Promise<void> {
    if (inFlight) return inFlight
    loading.value = !hasData.value
    refreshing.value = hasData.value
    error.value = null
    inFlight = getMetricsHistory(environmentId.value ?? undefined)
      .then((response) => {
        history.value = response.data ?? emptyHistory()
        lastUpdated.value = Date.now()
        failureCount.value = 0
      })
      .catch((reason) => {
        error.value = extractApiError(reason).message
        failureCount.value += 1
      })
      .finally(() => { loading.value = false; refreshing.value = false; inFlight = null })
    return inFlight
  }

  function nextDelay() { return failureCount.value ? Math.min(120_000, 5_000 * 2 ** Math.min(failureCount.value, 4)) : 10_000 }
  async function tick() { if (!document.hidden) await refresh(); if (polling) timer = setTimeout(tick, document.hidden ? 60_000 : nextDelay()) }
  function startPolling() { if (polling) return; polling = true; void tick() }
  function stopPolling() { polling = false; if (timer) clearTimeout(timer); timer = null }
  async function selectEnvironment(id: number | null) { if (environmentId.value === id) return; environmentId.value = id; await refresh() }

  return { history, loading, refreshing, error, lastUpdated, failureCount, environmentId, phase, refresh, startPolling, stopPolling, selectEnvironment }
})
