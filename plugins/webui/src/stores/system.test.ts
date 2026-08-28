import { describe, expect, test } from 'bun:test'
import { extrapolateUptime } from './uptime'

describe('system uptime extrapolation', () => {
  test('advances from the last server sample once per elapsed second', () => {
    expect(extrapolateUptime(120, 10_000, 13_999)).toBe(123)
  })

  test('does not run backwards when the local clock moves backwards', () => {
    expect(extrapolateUptime(120, 10_000, 9_000)).toBe(120)
  })

  test('keeps the server value when no synchronization timestamp exists', () => {
    expect(extrapolateUptime(120, null, 20_000)).toBe(120)
  })
})
