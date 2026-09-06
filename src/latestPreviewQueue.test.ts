import { expect, it } from 'vitest'
import { LatestPreviewQueue, PreviewSuperseded } from './latestPreviewQueue'

it('runs only active and latest pending, rejects stale output even when native cancellation is late', async () => {
  const queue = new LatestPreviewQueue<number>()
  let finish!: (value: number) => void
  let cancelled = 0
  const ran: number[] = []
  const first = queue.submit(() => new Promise<number>((resolve) => { finish = resolve; ran.push(0) }), () => { cancelled++ }).catch((error: unknown) => error)
  const second = queue.submit(async () => { ran.push(1); return 1 }, () => {}).catch((error: unknown) => error)
  const third = queue.submit(async () => { ran.push(2); return 2 }, () => {})
  expect(ran).toEqual([0]); expect(cancelled).toBe(2)
  finish(0)
  expect(await first).toBeInstanceOf(PreviewSuperseded)
  expect(await second).toBeInstanceOf(PreviewSuperseded)
  expect(await third).toBe(2); expect(ran).toEqual([0, 2])
})

it('isolates separate Before/After surfaces and recovers from failed work', async () => {
  const before = new LatestPreviewQueue<number>(); const after = new LatestPreviewQueue<number>()
  await expect(before.submit(async () => { throw Error('native error') }, () => {})).rejects.toThrow('native error')
  expect(await Promise.all([before.submit(async () => 1, () => {}), after.submit(async () => 2, () => {})])).toEqual([1, 2])
})
