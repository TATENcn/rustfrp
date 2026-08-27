import { apiGet, apiPost } from '@/api/client'
import type { ApiEnvelope, MetricsHistory, TrafficSample } from '@/api/types'

export function getMetricsHistory(environmentId?: number, limit = 360): Promise<ApiEnvelope<MetricsHistory>> {
  const query = new URLSearchParams({ limit: String(limit) })
  if (environmentId) query.set('environment_id', String(environmentId))
  return apiGet<MetricsHistory>(`/metrics/history?${query}`)
}

export function ingestTraffic(sample: TrafficSample): Promise<ApiEnvelope<null>> {
  return apiPost<null>('/metrics/traffic', sample)
}
