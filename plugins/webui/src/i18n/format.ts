import type { SupportedLocale } from './locale'

export type DurationStyle = 'long' | 'short' | 'narrow'

const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] as const

function finiteOrZero(value: number): number {
  return Number.isFinite(value) ? value : 0
}

export function formatNumber(
  value: number,
  locale: SupportedLocale,
  options: Intl.NumberFormatOptions = {},
): string {
  return new Intl.NumberFormat(locale, options).format(finiteOrZero(value))
}

export function formatPercent(value: number, locale: SupportedLocale): string {
  return new Intl.NumberFormat(locale, {
    style: 'percent',
    maximumFractionDigits: 1,
  }).format(finiteOrZero(value) / 100)
}

/**
 * Format a byte count using IEC (base-1024) units. Intl formats the numeric
 * portion only because its built-in byte units are SI and do not represent
 * KiB/MiB/GiB.
 */
export function formatBytes(value: number, locale: SupportedLocale): string {
  const bytes = Math.max(0, finiteOrZero(value))
  let scaled = bytes
  let unitIndex = 0

  while (scaled >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    scaled /= 1024
    unitIndex += 1
  }

  return `${formatNumber(scaled, locale, {
    maximumFractionDigits: unitIndex === 0 ? 0 : 1,
  })} ${BYTE_UNITS[unitIndex]}`
}

function durationParts(totalSeconds: number) {
  const total = Math.max(0, Math.floor(finiteOrZero(totalSeconds)))
  return {
    days: Math.floor(total / 86_400),
    hours: Math.floor((total % 86_400) / 3_600),
    minutes: Math.floor((total % 3_600) / 60),
    seconds: total % 60,
  }
}

function fallbackDuration(
  parts: ReturnType<typeof durationParts>,
  locale: SupportedLocale,
  includeSeconds: boolean,
): string {
  const values = [
    parts.days ? `${parts.days}${locale === 'zh' ? '天' : 'd'}` : '',
    parts.hours ? `${parts.hours}${locale === 'zh' ? '小时' : 'h'}` : '',
    parts.minutes ? `${parts.minutes}${locale === 'zh' ? '分钟' : 'm'}` : '',
    includeSeconds && (parts.seconds || (!parts.days && !parts.hours && !parts.minutes))
      ? `${parts.seconds}${locale === 'zh' ? '秒' : 's'}`
      : '',
  ].filter(Boolean)

  return values.join(locale === 'zh' ? '' : ' ')
}

/** Format daemon uptime with the selected UI locale and an older-browser fallback. */
export function formatDuration(
  totalSeconds: number,
  locale: SupportedLocale,
  options: { style?: DurationStyle; includeSeconds?: boolean } = {},
): string {
  const parts = durationParts(totalSeconds)
  const includeSeconds = options.includeSeconds ?? true
  const duration = includeSeconds
    ? parts
    : { days: parts.days, hours: parts.hours, minutes: parts.minutes }

  if (typeof Intl.DurationFormat !== 'function') {
    return fallbackDuration(parts, locale, includeSeconds)
  }

  return new Intl.DurationFormat(locale, {
    style: options.style ?? 'short',
    secondsDisplay:
      includeSeconds && !parts.days && !parts.hours && !parts.minutes
        ? 'always'
        : 'auto',
  }).format(duration)
}
