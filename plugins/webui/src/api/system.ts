import { apiGet, apiPost } from '@/api/client'
import type { ApiEnvelope, StatusResponse, ReloadResponse, ReloadTaskStatus, HealthResponse } from '@/api/types'

export function getStatus(): Promise<ApiEnvelope<StatusResponse>> {
  return apiGet<StatusResponse>('/status')
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
