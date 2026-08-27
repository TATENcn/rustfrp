import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { Environment } from '@/api/types'
import * as api from '@/api/environments'
import { extractApiError } from '@/api/errors'

export const useEnvironmentStore = defineStore('environments', () => {
  const environments = ref<Environment[]>([])
  const activeId = ref<number | null>(Number(localStorage.getItem('rustfrp-environment')) || null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const active = computed(() => environments.value.find((item) => item.id === activeId.value) ?? environments.value.find((item) => item.is_default) ?? null)

  async function fetchAll() {
    loading.value = true
    try {
      const response = await api.listEnvironments()
      environments.value = response.data ?? []
      if (!active.value) activeId.value = environments.value.find((item) => item.is_default)?.id ?? environments.value[0]?.id ?? null
      if (activeId.value) localStorage.setItem('rustfrp-environment', String(activeId.value))
    } catch (cause) {
      error.value = extractApiError(cause).message
    } finally {
      loading.value = false
    }
  }

  function select(id: number) {
    activeId.value = id
    localStorage.setItem('rustfrp-environment', String(id))
  }

  async function assignProfile(profileId: number, environmentId: number) {
    await api.assignProfile(profileId, environmentId)
    for (const environment of environments.value) {
      environment.profile_ids = environment.profile_ids.filter((id) => id !== profileId)
      if (environment.id === environmentId) environment.profile_ids.push(profileId)
    }
  }

  return { environments, activeId, active, loading, error, fetchAll, select, assignProfile }
})
