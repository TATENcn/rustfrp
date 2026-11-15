import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

// Two platform traps this script used to hit on Windows:
//   - `URL.pathname` keeps a leading slash before the drive letter (`/D:/a/...`),
//     which is not a valid path there;
//   - `Bun.Glob` yields backslash-separated relatives (`components\ui\x.tsx`),
//     so the forward-slash allow-lists below never matched.
// Normalise once: a real filesystem root, and POSIX-style relative keys.
const sourceRoot = new URL('../src/', import.meta.url)
const sourceDir = fileURLToPath(sourceRoot)
const allowedSvg = new Set<string>()
const allowedIconImports = new Set(['components/icon/AppIcon.tsx'])
const problems: string[] = []

for await (const entry of new Bun.Glob('**/*.{ts,tsx}').scan({ cwd: sourceDir })) {
  const relative = entry.replaceAll('\\', '/')
  const content = await Bun.file(join(sourceDir, relative)).text()
  if (/\p{Extended_Pictographic}/u.test(content)) problems.push(`${relative}: emoji are not allowed in UI source`)
  if (!allowedSvg.has(relative) && /<(?:svg|path)\b/i.test(content)) problems.push(`${relative}: inline UI SVG is not allowed; use AppIcon`)
  if (!allowedIconImports.has(relative) && !relative.startsWith('components/ui/') && /from ['"]lucide-react['"]/.test(content)) problems.push(`${relative}: Lucide imports must be registered in components/icon/AppIcon.tsx`)
  if (/import\s+\*\s+as\s+\w+\s+from\s+['"](?:@iconify|~icons)/.test(content)) problems.push(`${relative}: wildcard icon imports disable strict icon governance`)
}

if (problems.length) {
  console.error(problems.join('\n'))
  process.exit(1)
}

console.log('UI icon policy check passed')
