import { describe, expect, test } from 'bun:test'
import { formatBytes, formatDuration, formatNumber, formatPercent } from './format'

describe('locale-aware formatters', () => {
  test('formats and balances uptime into days', () => {
    expect(formatDuration(176_587, 'en')).toContain('2 days')
    expect(formatDuration(176_587, 'zh')).toContain('2天')
  })

  test('can omit seconds for compact status displays', () => {
    const value = formatDuration(3_667, 'en', { includeSeconds: false })
    expect(value).toContain('1 hr')
    expect(value).toContain('1 min')
    expect(value).not.toContain('7 sec')
  })

  test('formats zero uptime as seconds', () => {
    expect(formatDuration(0, 'en')).toContain('0 sec')
  })

  test('uses IEC units for byte counts', () => {
    expect(formatBytes(1_536, 'en')).toBe('1.5 KiB')
    expect(formatBytes(1_048_576, 'en')).toBe('1 MiB')
  })

  test('formats backend percent values from the zero-to-100 scale', () => {
    expect(formatPercent(12.34, 'en')).toBe('12.3%')
  })

  test('uses the selected locale for number grouping', () => {
    expect(formatNumber(12_345.6, 'en')).toBe('12,345.6')
  })
})
