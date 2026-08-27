import { apiDelete, apiGet, apiPost, apiPut } from '@/api/client'
import type { ApiEnvelope, Environment } from '@/api/types'

export const listEnvironments = (): Promise<ApiEnvelope<Environment[]>> =>
  apiGet<Environment[]>('/environments')

export const createEnvironment = (environment: Environment): Promise<ApiEnvelope<Environment>> =>
  apiPost<Environment>('/environments', environment)

export const updateEnvironment = (id: number, environment: Environment): Promise<ApiEnvelope<Environment>> =>
  apiPut<Environment>(`/environments/${id}`, environment)

export const deleteEnvironment = (id: number): Promise<ApiEnvelope<null>> =>
  apiDelete(`/environments/${id}`)

export const assignProfile = (profileId: number, environmentId: number): Promise<ApiEnvelope<null>> =>
  apiPut(`/profiles/${profileId}/environment`, { environment_id: environmentId })
