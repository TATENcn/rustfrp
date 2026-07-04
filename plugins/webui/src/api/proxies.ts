import { apiGet, apiPost, apiPut, apiDelete } from '@/api/client'
import type { ApiEnvelope, LocalProxy } from '@/api/types'

export function listProxies(): Promise<ApiEnvelope<LocalProxy[]>> {
  return apiGet<LocalProxy[]>('/proxies')
}

export function getProxy(id: number): Promise<ApiEnvelope<LocalProxy>> {
  return apiGet<LocalProxy>(`/proxies/${id}`)
}

export function createProxy(proxy: LocalProxy): Promise<ApiEnvelope<LocalProxy>> {
  return apiPost<LocalProxy>('/proxies', proxy)
}

export function updateProxy(id: number, proxy: LocalProxy): Promise<ApiEnvelope<LocalProxy>> {
  return apiPut<LocalProxy>(`/proxies/${id}`, proxy)
}

export function deleteProxy(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/proxies/${id}`)
}
