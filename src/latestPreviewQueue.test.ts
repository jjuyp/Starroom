import { describe, expect, it, vi } from 'vitest'
import { LatestPreviewQueue, PreviewSuperseded, previewFrameMayPublish } from './latestPreviewQueue'

const deferred = <T,>() => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => { resolve = yes })
  return { promise, resolve }
}

describe('LatestPreviewQueue', () => {
  it('cancels unmounted surfaces without running their pending work and remains reusable', async () => {
    const queue = new LatestPreviewQueue<number>(); const active = deferred<number>(); let pendingRuns = 0
    const first = queue.submit(() => active.promise, () => {}, 'a').catch((error: unknown) => error)
    const pending = queue.submit(async () => { pendingRuns++; return 2 }, () => {}, 'a').catch((error: unknown) => error)
    queue.cancelAll()
    expect(await first).toBeInstanceOf(PreviewSuperseded)
    expect(await pending).toBeInstanceOf(PreviewSuperseded)
    const restored = queue.submit(async () => 3, () => {})
    active.resolve(1)
    await expect(restored).resolves.toBe(3)
    expect(pendingRuns).toBe(0)
  })
  it('keeps publishing throughout a one-second drag faster than the renderer', async () => {
    vi.useFakeTimers(); vi.setSystemTime(0)
    try {
      const queue = new LatestPreviewQueue<number>(); const published: { value: number; at: number }[] = []
      const requests: Promise<unknown>[] = []
      const submit = (value: number, interactive: boolean) => queue.submit(
        () => new Promise<number>((resolve) => setTimeout(() => resolve(value), 100)), () => {}, interactive ? 'same-source' : undefined,
      ).then((value) => published.push({ value, at: Date.now() })).catch((error: unknown) => error)
      requests.push(submit(0, true))
      for (let value = 1; value <= 100; value++) {
        await vi.advanceTimersByTimeAsync(10)
        requests.push(submit(value, true))
      }
      expect(published[0].at).toBe(100)
      expect(published.filter((frame) => frame.at < 1000).length).toBeGreaterThanOrEqual(8)
      const final = submit(100, false)
      await vi.advanceTimersByTimeAsync(200)
      await final; await Promise.all(requests)
      expect(published.at(-1)?.value).toBe(100)
    } finally { vi.useRealTimers() }
  })
  it('does not let delayed JPEG decoding overwrite a newer frame or final state', () => {
    expect(previewFrameMayPublish(1, 0, 2, true, true)).toBe(true)
    expect(previewFrameMayPublish(1, 2, 2, true, true)).toBe(false)
    expect(previewFrameMayPublish(1, 0, 2, true, false)).toBe(false)
    expect(previewFrameMayPublish(1, 0, 2, false, false)).toBe(false)
    expect(previewFrameMayPublish(2, 1, 2, false, false)).toBe(true)
  })
  it('publishes completed interactive frames during continuous dragging, with one latest pending state', async () => {
    const queue = new LatestPreviewQueue<number>(); const active = deferred<number>(); let cancelled = 0
    const first = queue.submit(() => active.promise, () => { cancelled++ }, 'photo-a')
    const pending: Promise<unknown>[] = []
    for (let value = 1; value <= 1000; value++) pending.push(queue.submit(async () => value, () => {}, 'photo-a').catch((error: unknown) => error))
    active.resolve(0)
    await expect(first).resolves.toBe(0)
    const results = await Promise.all(pending)
    expect(cancelled).toBe(0)
    expect(results.at(-1)).toBe(1000)
    expect(results.slice(0, -1).every((value) => value instanceof PreviewSuperseded)).toBe(true)
  })

  it('final quality and source changes cancel interactive work instead of showing stale pixels', async () => {
    for (const nextKey of [undefined, 'photo-b']) {
      const queue = new LatestPreviewQueue<number>(); const active = deferred<number>(); let cancelled = 0
      const first = queue.submit(() => active.promise, () => { cancelled++ }, 'photo-a').catch((error: unknown) => error)
      const final = queue.submit(async () => 9, () => {}, nextKey)
      expect(await first).toBeInstanceOf(PreviewSuperseded)
      expect(cancelled).toBe(1)
      active.resolve(1)
      await expect(final).resolves.toBe(9)
    }
  })
  it('cancels active work and publishes only the latest state', async () => {
    const queue = new LatestPreviewQueue<number>(); const active = deferred<number>(); let cancelled = 0
    const first = queue.submit(() => active.promise, () => { cancelled++ }).catch((error: unknown) => error)
    const latest = queue.submit(async () => 2, () => {})
    expect(await first).toBeInstanceOf(PreviewSuperseded); expect(cancelled).toBe(1)
    active.resolve(1); await expect(latest).resolves.toBe(2)
  })

  it('never lets a stale completion publish', async () => {
    const queue = new LatestPreviewQueue<number>(); const active = deferred<number>(); const published: number[] = []
    const first = queue.submit(() => active.promise, () => {}).then((value) => published.push(value)).catch((error: unknown) => error)
    const latest = queue.submit(async () => 9, () => {}).then((value) => published.push(value))
    active.resolve(1); expect(await first).toBeInstanceOf(PreviewSuperseded); await latest
    expect(published).toEqual([9])
  })

  it('keeps 1000 updates bounded to active plus latest pending work', async () => {
    const queue = new LatestPreviewQueue<number>(); const active = deferred<number>(); let runs = 0
    const requests: Promise<unknown>[] = [queue.submit(() => { runs++; return active.promise }, () => {}).catch((error: unknown) => error)]
    for (let value = 1; value <= 1000; value++) requests.push(queue.submit(async () => { runs++; return value }, () => {}).catch((error: unknown) => error))
    active.resolve(0); const results = await Promise.all(requests)
    expect(runs).toBe(2); expect(results.at(-1)).toBe(1000)
    expect(results.slice(0, -1).every((value) => value instanceof PreviewSuperseded)).toBe(true)
  })

  it('isolates Before/After surfaces and recovers from errors', async () => {
    const before = new LatestPreviewQueue<number>(); const after = new LatestPreviewQueue<number>()
    await expect(before.submit(async () => { throw Error('native error') }, () => {})).rejects.toThrow('native error')
    await expect(Promise.all([before.submit(async () => 1, () => {}), after.submit(async () => 2, () => {})])).resolves.toEqual([1, 2])
  })
})
