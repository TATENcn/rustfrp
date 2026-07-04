import { apiGet, apiPost, apiPut, apiDelete } from '@/api/client'
import type { ApiEnvelope, FrpsProfile } from '@/api/types'

export function listProfiles(): Promise<ApiEnvelope<FrpsProfile[]>> {
  return apiGet<FrpsProfile[]>('/profiles')
}

export function getProfile(id: number): Promise<ApiEnvelope<FrpsProfile>> {
  return apiGet<FrpsProfile>(`/profiles/${id}`)
}

export function createProfile(profile: FrpsProfile): Promise<ApiEnvelope<FrpsProfile>> {
  return apiPost<FrpsProfile>('/profiles', profile)
}

export function updateProfile(id: number, profile: FrpsProfile): Promise<ApiEnvelope<FrpsProfile>> {
  return apiPut<FrpsProfile>(`/profiles/${id}`, profile)
}

export function deleteProfile(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/profiles/${id}`)
}
