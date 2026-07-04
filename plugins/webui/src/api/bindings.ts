import { apiGet, apiPost, apiPut, apiPatch, apiDelete } from '@/api/client'
import type { ApiEnvelope, BindingRule, BindingControlResponse } from '@/api/types'

export function listBindings(params?: {
  profile_id?: number
  proxy_id?: number
}): Promise<ApiEnvelope<BindingRule[]>> {
  const query = new URLSearchParams()
  if (params?.profile_id) query.set('profile_id', String(params.profile_id))
  if (params?.proxy_id) query.set('proxy_id', String(params.proxy_id))
  const qs = query.toString()
  return apiGet<BindingRule[]>(`/bindings${qs ? `?${qs}` : ''}`)
}

export function getBinding(id: number): Promise<ApiEnvelope<BindingRule>> {
  return apiGet<BindingRule>(`/bindings/${id}`)
}

export function createBinding(binding: BindingRule): Promise<ApiEnvelope<BindingRule>> {
  return apiPost<BindingRule>('/bindings', binding)
}

export function updateBinding(id: number, binding: BindingRule): Promise<ApiEnvelope<BindingRule>> {
  return apiPut<BindingRule>(`/bindings/${id}`, binding)
}

export function toggleBinding(id: number, enabled: boolean): Promise<ApiEnvelope<BindingRule>> {
  return apiPatch<BindingRule>(`/bindings/${id}/toggle`, { enabled })
}

export function startBinding(id: number): Promise<ApiEnvelope<BindingControlResponse>> {
  return apiPost<BindingControlResponse>(`/bindings/${id}/start`)
}

export function stopBinding(id: number): Promise<ApiEnvelope<BindingControlResponse>> {
  return apiPost<BindingControlResponse>(`/bindings/${id}/stop`)
}

export function deleteBinding(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/bindings/${id}`)
}
