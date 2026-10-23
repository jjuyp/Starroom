export interface NativeRequestTiming {
  queuedAt: number
  dispatchedAt: number
  receivedAt: number
}

export interface PreviewLatencySample {
  sequence: number
  phase: 'interactive' | 'final'
  editToDrawMs: number
  schedulingMs: number
  queueMs: number
  nativeRoundTripMs: number
  decodeAndDrawMs: number
  paintOpportunityMs?: number
}

/** Local, bounded diagnostics only. No paths, photo identifiers, settings or pixels.
 * editToDraw starts at the edit handler, not physical input; RAF is a browser paint
 * opportunity, not proof of OS presentation. Nothing is persisted or transmitted. */
export class PreviewLatencyTracker {
  /** Called from accepted edit handlers, never while deriving render output. */
  beginEdit(): number { return performance.now() }
  private sequence = 0
  private sampleSequence = 0
  private intents = new WeakMap<object, { sequence: number; at: number }>()
  private published = new WeakMap<object, { interactive: number; final: number }>()
  private mountedAt = new WeakMap<object, number>()
  private samples: PreviewLatencySample[] = []

  mark(photo: object, at: number): void {
    if (Number.isFinite(at)) this.intents.set(photo, { sequence: ++this.sequence, at })
  }

  mount(surface: object, at: number): void {
    this.mountedAt.set(surface, at)
    this.published.delete(surface)
  }

  drawn(photo: object, surface: object, phase: 'interactive' | 'final',
    timing: NativeRequestTiming, drawnAt: number): PreviewLatencySample | null {
    const intent = this.intents.get(photo)
    if (!intent || intent.at < (this.mountedAt.get(surface) ?? 0)) return null
    const timeline = [intent.at, timing.queuedAt, timing.dispatchedAt, timing.receivedAt, drawnAt]
    if (timeline.some((time, i) => !Number.isFinite(time) || i > 0 && time < timeline[i - 1])) return null
    const previous = this.published.get(surface) ?? { interactive: 0, final: 0 }
    if (intent.sequence <= previous[phase]) return null
    previous[phase] = intent.sequence
    this.published.set(surface, previous)
    const sample: PreviewLatencySample = {
      sequence: ++this.sampleSequence, phase,
      editToDrawMs: drawnAt - intent.at,
      schedulingMs: timing.queuedAt - intent.at,
      queueMs: timing.dispatchedAt - timing.queuedAt,
      nativeRoundTripMs: timing.receivedAt - timing.dispatchedAt,
      decodeAndDrawMs: drawnAt - timing.receivedAt,
    }
    this.samples.push(sample)
    if (this.samples.length > 256) this.samples.shift()
    return { ...sample }
  }

  paintOpportunity(sequence: number, phase: 'interactive' | 'final', elapsedMs: number): void {
    const sample = this.samples.find((entry) => entry.sequence === sequence && entry.phase === phase)
    if (sample && Number.isFinite(elapsedMs) && elapsedMs >= sample.editToDrawMs) sample.paintOpportunityMs = elapsedMs
  }

  snapshot(): PreviewLatencySample[] { return this.samples.map((sample) => ({ ...sample })) }
}

export const previewLatency = new PreviewLatencyTracker()
