import { ofetch } from 'ofetch'
import type { ApiEnvelope } from '@/api/types'

const API_BASE = '/api/v1'

export const apiClient = ofetch.create({
  baseURL: API_BASE,
  headers: {
    'Content-Type': 'application/json',
  },
  onRequest({ options }) {
    const token = localStorage.getItem('api_token')
    if (token) {
      options.headers = {
        ...options.headers,
        Authorization: `Bearer ${token}`,
      }
    }
  },
  onResponseError({ response }) {
    if (response.status === 401) {
      localStorage.removeItem('api_token')
      window.location.hash = '#/login'
    }
  },
})

// Typed API helpers
export async function apiGet<T>(path: string): Promise<ApiEnvelope<T>> {
  return apiClient<ApiEnvelope<T>>(path)
}

export async function apiPost<T>(path: string, body?: unknown): Promise<ApiEnvelope<T>> {
  return apiClient<ApiEnvelope<T>>(path, { method: 'POST', body })
}

export async function apiPut<T>(path: string, body?: unknown): Promise<ApiEnvelope<T>> {
  return apiClient<ApiEnvelope<T>>(path, { method: 'PUT', body })
}

export async function apiPatch<T>(path: string, body?: unknown): Promise<ApiEnvelope<T>> {
  return apiClient<ApiEnvelope<T>>(path, { method: 'PATCH', body })
}

export async function apiDelete(path: string): Promise<ApiEnvelope<null>> {
  return apiClient<ApiEnvelope<null>>(path, { method: 'DELETE' })
}
