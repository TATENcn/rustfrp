const sourceRoot = new URL('../src/', import.meta.url)
const allowedSvg = new Set<string>()
const allowedIconImports = new Set(['components/icon/AppIcon.tsx'])
const problems: string[] = []

for await (const relative of new Bun.Glob('**/*.{ts,tsx}').scan({ cwd: sourceRoot.pathname })) {
  const content = await Bun.file(new URL(relative, sourceRoot)).text()
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
