import { describe, expect, it } from 'vitest'
import { LatestPreviewQueue, PreviewSuperseded } from './latestPreviewQueue'

const deferred = <T,>() => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => { resolve = yes })
  return { promise, resolve }
}

describe('LatestPreviewQueue', () => {
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
