import { apiGet, apiPut } from '@/api/client'
import type { ApiEnvelope, Environment } from '@/api/types'

// The WebUI ships a switcher and per-profile reassignment only; creating,
// renaming, and deleting environments stays an API/operator action, so no
// client bindings exist for those routes.
export const listEnvironments = (): Promise<ApiEnvelope<Environment[]>> =>
  apiGet<Environment[]>('/environments')

export const assignProfile = (profileId: number, environmentId: number): Promise<ApiEnvelope<null>> =>
  apiPut(`/profiles/${profileId}/environment`, { environment_id: environmentId })
