// UI request lifetimes only. Native providers remain authoritative for image processing.
export class EditorRequestGate {
  private epoch = 0
  private lanes = new Map<string, number>()
  invalidate() { this.epoch++; this.lanes.clear() }
  begin(lane: string): () => boolean {
    const epoch = this.epoch
    const generation = (this.lanes.get(lane) ?? 0) + 1
    this.lanes.set(lane, generation)
    return () => this.epoch === epoch && this.lanes.get(lane) === generation
  }
}

export function appendWithinCapacity<T>(existing: readonly T[], incoming: readonly T[], limit: number):
  { ok: true; values: T[] } | { ok: false; remaining: number } {
  if (existing.length + incoming.length > limit) return { ok: false, remaining: Math.max(0, limit - existing.length) }
  return { ok: true, values: [...existing, ...incoming] }
}
