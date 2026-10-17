export class PreviewSuperseded extends Error {
  constructor() { super('PreviewCancelled: superseded'); this.name = 'PreviewSuperseded' }
}

/** Native completion is followed by asynchronous JPEG decode. Keep that second boundary ordered. */
export function previewFrameMayPublish(generation: number, published: number, latest: number,
  interactive: boolean, latestInteractive: boolean): boolean {
  return generation > published && (generation === latest || interactive && latestInteractive)
}

interface Request<T> {
  generation: number
  run: () => Promise<T>
  cancel: () => void
  resolve: (value: T) => void
  reject: (error: unknown) => void
  settled: boolean
  interactiveKey?: string
}

/** One running render and one replaceable pending state per visible preview surface. */
export class LatestPreviewQueue<T> {
  private active: Request<T> | null = null
  private pending: Request<T> | null = null
  private generation = 0
  private interactiveKey: string | undefined

  cancelAll(): void {
    this.generation++
    this.interactiveKey = undefined
    if (this.active) this.supersede(this.active)
    if (this.pending) this.supersede(this.pending)
    this.pending = null
  }

  submit(run: () => Promise<T>, cancel: () => void, interactiveKey?: string): Promise<T> {
    const generation = ++this.generation
    this.interactiveKey = interactiveKey
    return new Promise<T>((resolve, reject) => {
      const request: Request<T> = { generation, run, cancel, resolve, reject, settled: false, interactiveKey }
      if (!this.active) { void this.execute(request); return }
      // During a same-source drag, finish and display the in-flight intermediate frame.
      // Cancelling it on every pointer event starves publication whenever input is faster
      // than rendering. Final-quality/source switches still retain strict latest-only semantics.
      if (!interactiveKey || this.active.interactiveKey !== interactiveKey) this.supersede(this.active)
      if (this.pending) this.supersede(this.pending)
      this.pending = request
    })
  }

  private supersede(request: Request<T>) {
    if (request.settled) return
    request.settled = true
    request.cancel()
    request.reject(new PreviewSuperseded())
  }

  private async execute(request: Request<T>) {
    this.active = request
    try {
      const value = await request.run()
      if (!request.settled && (request.generation === this.generation
        || request.interactiveKey !== undefined && request.interactiveKey === this.interactiveKey)) {
        request.settled = true
        request.resolve(value)
      } else if (!request.settled) this.supersede(request)
    } catch (error) {
      if (!request.settled) { request.settled = true; request.reject(error) }
    }
    finally {
      if (this.active === request) this.active = null
      const next = this.pending; this.pending = null
      if (next && !next.settled) void this.execute(next)
    }
  }
}
