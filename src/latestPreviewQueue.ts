export class PreviewSuperseded extends Error {
  constructor() { super('PreviewCancelled: superseded'); this.name = 'PreviewSuperseded' }
}

interface Request<T> { run: () => Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void }

/** One running render and one replaceable pending state per visible preview surface. */
export class LatestPreviewQueue<T> {
  private running: Request<T> | null = null
  private pending: Request<T> | null = null

  submit(run: () => Promise<T>, cancel: () => void): Promise<T> {
    void cancel // Kept for API compatibility; the active frame is intentionally allowed to finish.
    return new Promise<T>((resolve, reject) => {
      const request = { run, resolve, reject }
      if (this.running) {
        this.pending?.reject(new PreviewSuperseded())
        this.pending = request
      } else { void this.execute(request) }
    })
  }

  private async execute(request: Request<T>) {
    this.running = request
    try {
      const value = await request.run()
      request.resolve(value)
    } catch (error) { request.reject(error) }
    finally {
      this.running = null
      const next = this.pending; this.pending = null
      if (next) void this.execute(next)
    }
  }
}
