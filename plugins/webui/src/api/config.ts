import { apiPost } from '@/api/client'
import type { ApiEnvelope } from '@/api/types'

export interface Rename {
  kind: 'profile' | 'proxy' | 'visitor'
  from: string
  to: string
}

export interface ImportSummary {
  profile_id: number
  profile_name: string
  proxies_imported: number
  visitors_imported: number
  renamed_items: Rename[]
}

export function importFrpcToml(
  profileName: string,
  toml: string,
): Promise<ApiEnvelope<ImportSummary>> {
  return apiPost<ImportSummary>('/config/import', {
    profile_name: profileName,
    toml,
  })
}

export async function downloadBackup(): Promise<void> {
  const headers: Record<string, string> = {}
  const token = localStorage.getItem('api_token')
  if (token) headers.Authorization = `Bearer ${token}`
  const response = await fetch('/api/v1/config/export', { headers })
  if (!response.ok) throw new Error(`Backup export failed: HTTP ${response.status}`)
  const disposition = response.headers.get('content-disposition') ?? ''
  const filename = disposition.match(/filename="([^"]+)"/)?.[1] ?? 'rustfrp-backup.sqlite'
  const url = URL.createObjectURL(await response.blob())
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  URL.revokeObjectURL(url)
}
