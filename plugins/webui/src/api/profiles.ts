import { apiGet, apiPost, apiPut, apiDelete } from '@/api/client'
import type { ApiEnvelope, BindingRule, FrpsProfile, ProfileRuntimeResponse } from '@/api/types'

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

export function replaceProfileProxies(id: number, proxyIds: number[]): Promise<ApiEnvelope<BindingRule[]>> {
  return apiPut<BindingRule[]>(`/profiles/${id}/proxies`, { proxy_ids: proxyIds })
}

export function getProfileRuntime(id: number): Promise<ApiEnvelope<ProfileRuntimeResponse>> {
  return apiGet<ProfileRuntimeResponse>(`/profiles/${id}/runtime`)
}

export function startProfile(id: number): Promise<ApiEnvelope<ProfileRuntimeResponse>> {
  return apiPost<ProfileRuntimeResponse>(`/profiles/${id}/start`)
}

export function stopProfile(id: number): Promise<ApiEnvelope<ProfileRuntimeResponse>> {
  return apiPost<ProfileRuntimeResponse>(`/profiles/${id}/stop`)
}
