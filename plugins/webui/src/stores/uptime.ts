const SECOND_MS = 1_000

export function extrapolateUptime(baseSeconds: number, syncedAt: number | null, now: number): number {
  if (syncedAt === null) return baseSeconds
  return baseSeconds + Math.max(0, Math.floor((now - syncedAt) / SECOND_MS))
}
