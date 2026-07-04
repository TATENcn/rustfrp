/**
 * i18n key consistency checker.
 *
 * Verifies:
 * 1. en.json and zh.json have identical key sets.
 * 2. All `t('...')` calls in source files reference known keys.
 * 3. Reports keys in translation files that are never used in code.
 *
 * Run with: bun scripts/check-i18n-keys.ts
 */

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, extname } from 'node:path'

const ROOT = join(import.meta.dirname, '..')
const SRC_DIR = join(ROOT, 'src')
const I18N_DIR = join(SRC_DIR, 'i18n', 'messages')

// ── 1. Load and validate translation files ──

function loadJson(path: string): Record<string, string> {
  try {
    const raw = readFileSync(path, 'utf-8')
    return JSON.parse(raw)
  } catch (err) {
    console.error(`ERROR: Failed to parse ${path}:`, err)
    process.exit(1)
  }
}

const enKeys = Object.keys(loadJson(join(I18N_DIR, 'en.json'))).sort()
const zhKeys = Object.keys(loadJson(join(I18N_DIR, 'zh.json'))).sort()

console.log(`en.json: ${enKeys.length} keys`)
console.log(`zh.json: ${zhKeys.length} keys`)

let errors = 0

// Check key set equality
const enMissing = zhKeys.filter((k) => !enKeys.includes(k))
const zhMissing = enKeys.filter((k) => !zhKeys.includes(k))

if (enMissing.length > 0) {
  console.error(`ERROR: Keys in zh.json but missing from en.json: ${enMissing.join(', ')}`)
  errors++
}
if (zhMissing.length > 0) {
  console.error(`ERROR: Keys in en.json but missing from zh.json: ${zhMissing.join(', ')}`)
  errors++
}

// ── 2. Scan source files for t('...') calls ──

function collectFiles(dir: string): string[] {
  const result: string[] = []
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (entry === 'node_modules' || entry === 'dist') continue
    const stat = statSync(full)
    if (stat.isDirectory()) {
      result.push(...collectFiles(full))
    } else if (['.vue', '.ts'].includes(extname(entry))) {
      result.push(full)
    }
  }
  return result
}

const sourceFiles = collectFiles(SRC_DIR)
const tCallRegex = /\bt\s*\(\s*['"]([^'"]+)['"]/g
const usedKeys = new Set<string>()

for (const file of sourceFiles) {
  const content = readFileSync(file, 'utf-8')
  let match: RegExpExecArray | null
  while ((match = tCallRegex.exec(content)) !== null) {
    usedKeys.add(match[1]!)
  }
}

console.log(`Source files scanned: ${sourceFiles.length}`)
console.log(`Unique t() keys referenced: ${usedKeys.size}`)

// ── 3. Report missing keys in source ──

const missingInSource = [...usedKeys].filter((k) => !enKeys.includes(k))
if (missingInSource.length > 0) {
  console.error(`ERROR: Keys used in source but not defined in en.json: ${missingInSource.join(', ')}`)
  errors++
}

// ── 4. Report unused keys ──

const unusedInCode = enKeys.filter((k) => !usedKeys.has(k))
if (unusedInCode.length > 0) {
  console.warn(`WARNING: Keys in en.json but never used in code: ${unusedInCode.join(', ')}`)
}

// ── Result ──

if (errors === 0) {
  console.log('✓ i18n key check passed')
} else {
  console.error(`✗ i18n key check failed with ${errors} error(s)`)
  process.exit(1)
}
