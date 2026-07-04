import { apiGet, apiPost, apiPut, apiDelete } from '@/api/client'
import type { ApiEnvelope, LocalVisitor } from '@/api/types'

export function listVisitors(): Promise<ApiEnvelope<LocalVisitor[]>> {
  return apiGet<LocalVisitor[]>('/visitors')
}

export function getVisitor(id: number): Promise<ApiEnvelope<LocalVisitor>> {
  return apiGet<LocalVisitor>(`/visitors/${id}`)
}

export function createVisitor(visitor: LocalVisitor): Promise<ApiEnvelope<LocalVisitor>> {
  return apiPost<LocalVisitor>('/visitors', visitor)
}

export function updateVisitor(id: number, visitor: LocalVisitor): Promise<ApiEnvelope<LocalVisitor>> {
  return apiPut<LocalVisitor>(`/visitors/${id}`, visitor)
}

export function deleteVisitor(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/visitors/${id}`)
}
