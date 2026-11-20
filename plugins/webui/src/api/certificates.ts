import { apiDelete, apiGet, apiPost, apiPut } from '@/api/client'
import type { ApiEnvelope } from '@/api/types'

/**
 * Certificate metadata as returned by the daemon.
 *
 * This type intentionally has no certificate or key fields: the daemon never
 * sends them, and there is no endpoint to fetch them.
 *
 * A certificate exists before it has material, which is why the fields that
 * only a bundle can supply are nullable: `material_id` is `null` until the first
 * import, and `days_remaining` follows it.
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
  /** Material row these fields came from; null before the first import. */
  material_id: number | null
  days_remaining: number | null
  /** Derived state. `usable` is exactly `status !== expired|not_yet_valid|pending`. */
  status: 'active' | 'expiring_soon' | 'expired' | 'not_yet_valid' | 'pending'
  /** Why it cannot be used, when it cannot. */
  status_reason: string | null
  usable: boolean
  /** Last failure on any attached channel, so a pending certificate can explain itself. */
  channel_error: string | null
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

/** How a certificate gets updated. */
export type SourceKind = 'push' | 'certd' | 'url'

/**
 * One channel for one certificate.
 *
 * A channel can be unbound (`certificate_id: null`) until it is attached; that is
 * what the legacy global tokens became after the upgrade.
 */
export interface CertificateSource {
  id: number
  tenant_id: string
  certificate_id: number | null
  kind: SourceKind
  name: string
  domain: string
  base_url: string
  /** Allowed domains, for a push token only. */
  domains: string[]
  /** Whether a credential is stored; the credential itself is never sent. */
  has_secret: boolean
  is_primary: boolean
  enabled: boolean
  expires_at: string | null
  revoked_at: string | null
  last_used_at: string | null
  use_count: number
  last_run_at: string | null
  last_status: string | null
  last_error: string | null
  next_run_at: string | null
  created_at: string
  updated_at: string
}

export interface SourceInput {
  certificate_id?: number | null
  kind: SourceKind
  name: string
  domain?: string
  base_url?: string
  domains?: string[]
  key_id?: string
  key_secret?: string
  enabled?: boolean
}

export interface CreatedSource {
  source: CertificateSource
  /** Shown once, for a push channel only. */
  plaintext: string | null
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
  applied: number
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
  return apiDelete(`/certificate-bindings/${id}?remove_files=${removeFiles}`)
}

// === Channels ===

export function listSources(certificateId?: number): Promise<ApiEnvelope<CertificateSource[]>> {
  const qs =
    certificateId === undefined ? '' : `?certificate_id=${certificateId}`
  return apiGet<CertificateSource[]>(`/certificate-sources${qs}`)
}

export function createSource(body: SourceInput): Promise<ApiEnvelope<CreatedSource>> {
  return apiPost<CreatedSource>('/certificate-sources', body)
}

export function deleteSource(id: number): Promise<ApiEnvelope<null>> {
  return apiDelete(`/certificate-sources/${id}`)
}

/** Attach to a certificate, or detach with `null`. */
export function attachSource(
  id: number,
  certificateId: number | null,
): Promise<ApiEnvelope<CertificateSource>> {
  return apiPut<CertificateSource>(`/certificate-sources/${id}/certificate`, {
    certificate_id: certificateId,
  })
}

export function promoteSource(id: number): Promise<ApiEnvelope<CertificateSource>> {
  return apiPost<CertificateSource>(`/certificate-sources/${id}/primary`)
}

export function runSource(id: number): Promise<ApiEnvelope<PullReport>> {
  return apiPost<PullReport>(`/certificate-sources/${id}/run`)
}
