import { apiGet, apiPost } from '@/api/client'
import type { ApiEnvelope, MetricsHistory, TrafficSample } from '@/api/types'

export function getMetricsHistory(limit = 36_000): Promise<ApiEnvelope<MetricsHistory>> {
  return apiGet<MetricsHistory>(`/metrics/history?limit=${limit}`)
}

export function ingestTraffic(sample: TrafficSample): Promise<ApiEnvelope<null>> {
  return apiPost<null>('/metrics/traffic', sample)
}
