export class PreviewSuperseded extends Error {
  constructor() { super('PreviewCancelled: superseded'); this.name = 'PreviewSuperseded' }
}

interface Request<T> { run: () => Promise<T>; cancel: () => void; resolve: (value: T) => void; reject: (error: unknown) => void; stale: boolean }

/** One running render and one replaceable pending state per visible preview surface. */
export class LatestPreviewQueue<T> {
  private running: Request<T> | null = null
  private pending: Request<T> | null = null

  submit(run: () => Promise<T>, cancel: () => void): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const request = { run, cancel, resolve, reject, stale: false }
      if (this.running) {
        this.running.stale = true
        this.running.cancel()
        this.pending?.reject(new PreviewSuperseded())
        this.pending = request
      } else { void this.execute(request) }
    })
  }

  private async execute(request: Request<T>) {
    this.running = request
    try {
      const value = await request.run()
      if (request.stale) request.reject(new PreviewSuperseded())
      else request.resolve(value)
    } catch (error) { request.reject(request.stale ? new PreviewSuperseded() : error) }
    finally {
      this.running = null
      const next = this.pending; this.pending = null
      if (next) void this.execute(next)
    }
  }
}
