export class PreviewSuperseded extends Error {
  constructor() { super('PreviewCancelled: superseded'); this.name = 'PreviewSuperseded' }
}

interface Request<T> {
  generation: number
  run: () => Promise<T>
  cancel: () => void
  resolve: (value: T) => void
  reject: (error: unknown) => void
  settled: boolean
}

/** One running render and one replaceable pending state per visible preview surface. */
export class LatestPreviewQueue<T> {
  private active: Request<T> | null = null
  private pending: Request<T> | null = null
  private generation = 0

  submit(run: () => Promise<T>, cancel: () => void): Promise<T> {
    const generation = ++this.generation
    return new Promise<T>((resolve, reject) => {
      const request: Request<T> = { generation, run, cancel, resolve, reject, settled: false }
      if (!this.active) { void this.execute(request); return }
      this.supersede(this.active)
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
      if (!request.settled && request.generation === this.generation) {
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
