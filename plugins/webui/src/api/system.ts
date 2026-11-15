import { apiGet, apiPost } from '@/api/client'
import type { ApiEnvelope, StatusResponse, ReloadResponse, ReloadTaskStatus, HealthResponse } from '@/api/types'

export function getStatus(environmentId?: number | null): Promise<ApiEnvelope<StatusResponse>> {
  const query = environmentId ? `?environment_id=${environmentId}` : ''
  return apiGet<StatusResponse>(`/status${query}`)
}

export function triggerReload(): Promise<ApiEnvelope<ReloadResponse>> {
  return apiPost<ReloadResponse>('/reload')
}

export function getReloadStatus(taskId: string): Promise<ApiEnvelope<ReloadTaskStatus>> {
  return apiGet<ReloadTaskStatus>(`/reload/${taskId}`)
}

export function healthCheck(): Promise<ApiEnvelope<HealthResponse>> {
  return apiGet<HealthResponse>('/health')
}
