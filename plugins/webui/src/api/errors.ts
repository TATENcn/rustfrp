import type { I18nKey } from '@/i18n/types'

// Backend error code → frontend i18n key
const ERROR_CODE_MAP: Record<string, I18nKey> = {
  DB_001: 'error.serverError',
  DB_004: 'error.serverError',
  AUTH_001: 'error.unauthorized',
  CFG_001: 'error.validation',
  PROC_001: 'error.serverError',
  NET_001: 'error.network',
  SYS_001: 'error.serverError',
}

export function resolveErrorMessage(
  apiCode: string | undefined,
  fallback: I18nKey = 'error.serverError',
): I18nKey {
  if (!apiCode) return fallback
  return ERROR_CODE_MAP[apiCode] ?? fallback
}

export class AppError extends Error {
  code?: string
  userMessageKey?: string

  constructor(message: string, code?: string, userMessageKey?: string) {
    super(message)
    this.name = 'AppError'
    this.code = code
    this.userMessageKey = userMessageKey
  }
}

export function extractApiError(err: unknown): AppError {
  if (err instanceof AppError) return err
  if (err && typeof err === 'object' && 'data' in (err as Record<string, unknown>)) {
    const apiErr = (err as { data?: { error?: { code?: string; message?: string; user_message_key?: string } } }).data?.error
    if (apiErr) {
      return new AppError(apiErr.message || 'Unknown error', apiErr.code, apiErr.user_message_key)
    }
  }
  return new AppError(String(err))
}
