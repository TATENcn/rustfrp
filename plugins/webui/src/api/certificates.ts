import { apiDelete, apiGet, apiPost } from '@/api/client'
import type { ApiEnvelope } from '@/api/types'

/**
 * Certificate metadata as returned by the daemon.
 *
 * This type intentionally has no certificate or key fields: the daemon never
 * sends them, and there is no endpoint to fetch them.
 */
export interface Certificate {
  id: number
  tenant_id: string
  primary_domain: string
  sans: string[]
  issuer: string
  serial: string
  fingerprint_sha256: string
  key_algo: string
  not_before: string
  not_after: string
  days_remaining: number
  source: string
  source_detail: string | null
  created_at: string
  updated_at: string
  bindings: CertificateBinding[]
  warnings: string[]
}

export interface CertificateBinding {
  id: number
  tenant_id: string
  cert_id: number
  proxy_id: number
  proxy_name: string
  applied_fingerprint: string | null
  applied_at: string | null
  last_error: string | null
  created_at: string
  updated_at: string
}

export interface IngestToken {
  id: number
  tenant_id: string
  name: string
  domains: string[]
  expires_at: string | null
  revoked_at: string | null
  last_used_at: string | null
  use_count: number
  created_at: string
}

export interface CreatedIngestToken {
  token: IngestToken
  /** Present only in the creation response. */
  plaintext: string | null
}

export type PullKind = 'certd' | 'url'

export interface PullSource {
  id: number
  tenant_id: string
  name: string
  kind: PullKind
  base_url: string
  domain: string
  key_id?: string | null
  has_secret: boolean
  enabled: boolean
  last_run_at: string | null
  last_status: string | null
  last_error: string | null
  created_at: string
  updated_at: string
}

export interface PullSourceInput {
  name: string
  kind: PullKind
  base_url: string
  domain: string
  key_id?: string
  key_secret?: string
  enabled?: boolean
}

export interface ApplyReport {
  applied: number[]
  failed: { binding_id: number; error: string }[]
}

export interface PullReport {
  source_id: number
  status: string
  certificate_id: number | null
  primary_domain: string | null
  message: string
}

export function listCertificates(params?: {
  expiring_within?: number
  domain?: string
}): Promise<ApiEnvelope<Certificate[]>> {
  const query = new URLSearchParams()
  if (params?.expiring_within !== undefined)
    query.set('expiring_within', String(params.expiring_within))
  if (params?.domain) query.set('domain', params.domain)
  const qs = query.toString()
  return apiGet<Certificate[]>(`/certificates${qs ? `?${qs}` : ''}`)
}

export function importPem(body: {
  cert: string
  key: string
  note?: string
}): Promise<ApiEnvelope<Certificate>> {
  return apiPost<Certificate>('/certificates/pem', body)
}

export function importFromPath(body: {
  crt_path: string
  key_path: string
  note?: string
}): Promise<ApiEnvelope<Certificate>> {
  return apiPost<Certificate>('/certificates/from-path', body)
}

export function applyCertificate(id: number): Promise<ApiEnvelope<ApplyReport>> {
  return apiPost<ApplyReport>(`/certificates/${id}/apply`)
}

export function deleteCertificate(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/certificates/${id}`)
}

export function bindCertificate(body: {
  cert_id: number
  proxy_id: number
}): Promise<ApiEnvelope<CertificateBinding>> {
  return apiPost<CertificateBinding>('/certificate-bindings', body)
}

export function unbindCertificate(
  id: number,
  removeFiles = false,
): Promise<ApiEnvelope<null>> {
  return apiDelete(`/certificate-bindings/${id}${removeFiles ? '?remove_files=true' : ''}`)
}

export function listIngestTokens(): Promise<ApiEnvelope<IngestToken[]>> {
  return apiGet<IngestToken[]>('/cert-ingest-tokens')
}

export function createIngestToken(body: {
  name: string
  domains: string[]
  expires_at?: string | null
}): Promise<ApiEnvelope<CreatedIngestToken>> {
  return apiPost<CreatedIngestToken>('/cert-ingest-tokens', body)
}

export function deleteIngestToken(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/cert-ingest-tokens/${id}`)
}

export function listPullSources(): Promise<ApiEnvelope<PullSource[]>> {
  return apiGet<PullSource[]>('/cert-pull-sources')
}

export function createPullSource(body: PullSourceInput): Promise<ApiEnvelope<PullSource>> {
  return apiPost<PullSource>('/cert-pull-sources', body)
}

export function deletePullSource(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/cert-pull-sources/${id}`)
}

export function runPullSource(id: number): Promise<ApiEnvelope<PullReport>> {
  return apiPost<PullReport>(`/cert-pull-sources/${id}/run`)
}
