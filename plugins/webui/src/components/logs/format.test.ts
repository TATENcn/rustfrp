import { describe, expect, test } from 'bun:test'
import { classifyLogLevel, formatLogLines } from './format'

describe('log formatting', () => {
  test('renders ANSI colors and removes control sequences from output', () => {
    const [line] = formatLogLines('\u001b[31mfailed\u001b[0m')
    expect(line.html).toContain('class="ansi-red-fg"')
    expect(line.html).not.toContain('\u001b[')
  })

  test('escapes log-provided HTML before it is rendered', () => {
    const [line] = formatLogLines('<script>alert(1)</script>')
    expect(line.html).toContain('&lt;script&gt;')
    expect(line.html).not.toContain('<script>')
  })

  test('keeps semantic level colors as a fallback for plain logs', () => {
    expect(classifyLogLevel('WARN reconnecting')).toBe('warn')
    expect(classifyLogLevel('connection failed')).toBe('error')
  })
})
