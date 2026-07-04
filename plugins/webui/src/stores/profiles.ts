import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { FrpsProfile } from '@/api/types'
import * as profilesApi from '@/api/profiles'
import { extractApiError } from '@/api/errors'

export const useProfileStore = defineStore('profiles', () => {
  const profiles = ref<FrpsProfile[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetchAll() {
    loading.value = true
    error.value = null
    try {
      const resp = await profilesApi.listProfiles()
      profiles.value = resp.data ?? []
    } catch (e) {
      error.value = extractApiError(e).message
      throw e
    } finally {
      loading.value = false
    }
  }

  async function create(profile: FrpsProfile): Promise<FrpsProfile> {
    const resp = await profilesApi.createProfile(profile)
    if (resp.data) {
      profiles.value.push(resp.data)
    }
    return resp.data!
  }

  async function update(id: number, profile: FrpsProfile): Promise<FrpsProfile> {
    const resp = await profilesApi.updateProfile(id, profile)
    if (resp.data) {
      const idx = profiles.value.findIndex((p) => p.id === id)
      if (idx !== -1) profiles.value[idx] = resp.data
    }
    return resp.data!
  }

  async function remove(id: number) {
    await profilesApi.deleteProfile(id)
    profiles.value = profiles.value.filter((p) => p.id !== id)
  }

  return { profiles, loading, error, fetchAll, create, update, remove }
})
