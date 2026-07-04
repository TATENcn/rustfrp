import { apiGet } from './client'
import type { ApiEnvelope, LogResponse } from './types'

export interface LogQueryParams {
  lines?: number // default: 200, max: 1000
  log_type?: 'combined' | 'stdout' | 'stderr' // default: 'combined'
}

/**
 * Get logs for a specific profile
 */
export async function getLogs(
  profileId: number,
  params: LogQueryParams = {},
): Promise<ApiEnvelope<LogResponse>> {
  const query = new URLSearchParams()
  if (params.lines) {
    query.set('lines', String(params.lines))
  }
  if (params.log_type) {
    query.set('log_type', params.log_type)
  }
  const queryString = query.toString()
  const path = queryString ? `logs/${profileId}?${queryString}` : `logs/${profileId}`
  return apiGet<LogResponse>(path)
}