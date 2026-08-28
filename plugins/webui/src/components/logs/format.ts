import { AnsiUp } from 'ansi_up'

export type LogLevel = 'error' | 'warn' | 'debug' | 'info'

export interface FormattedLogLine {
  id: string
  text: string
  html: string
  level: LogLevel
  number: number
}

export function classifyLogLevel(text: string): LogLevel {
  if (/\berror|fatal|failed\b/i.test(text)) return 'error'
  if (/\bwarn(?:ing)?\b/i.test(text)) return 'warn'
  if (/\bdebug|trace\b/i.test(text)) return 'debug'
  return 'info'
}

export function formatLogLines(content: string): FormattedLogLine[] {
  const ansi = new AnsiUp()
  ansi.escape_html = true
  ansi.use_classes = true
  ansi.url_allowlist = { http: 1, https: 1 }

  return content.split('\n').map((text, index) => ({
    id: `${index}-${text.slice(0, 24)}`,
    text,
    html: ansi.ansi_to_html(text),
    level: classifyLogLevel(text),
    number: index + 1,
  }))
}
