import { apiDelete, apiGet, apiPost } from '@/api/client'
import type {
  ApiEnvelope,
  AvailableFrpVersion,
  FrpVersionList,
  InstalledFrpVersion,
} from '@/api/types'

export function listFrpVersions(): Promise<ApiEnvelope<FrpVersionList>> {
  return apiGet<FrpVersionList>('/frp/versions')
}

export function listAvailableFrpVersions(): Promise<ApiEnvelope<AvailableFrpVersion[]>> {
  return apiGet<AvailableFrpVersion[]>('/frp/releases')
}

export function installFrpVersion(
  version: string,
  mirrorBase?: string,
): Promise<ApiEnvelope<InstalledFrpVersion>> {
  return apiPost<InstalledFrpVersion>('/frp/versions', {
    version,
    mirror_base: mirrorBase || null,
  })
}

export function activateFrpVersion(version: string): Promise<ApiEnvelope<FrpVersionList>> {
  return apiPost<FrpVersionList>(`/frp/versions/${encodeURIComponent(version)}/activate`)
}

export function deleteFrpVersion(version: string): Promise<ApiEnvelope<null>> {
  return apiDelete(`/frp/versions/${encodeURIComponent(version)}`)
}
