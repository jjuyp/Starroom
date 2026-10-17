import { expect, it } from 'vitest'
import { loadProgressiveThumbnails } from './progressiveThumbnails'

it('publishes each result progressively, bounds workers and isolates failures', async () => {
  let pending = 0; let peak = 0
  const published: number[] = []; const failed: number[] = []
  await loadProgressiveThumbnails(Array.from({ length: 100 }, (_, i) => i), async (id) => {
    pending++; peak = Math.max(peak, pending)
    await Promise.resolve(); pending--
    if (id === 11) throw Error('corrupt')
    return `cache:${id}`
  }, (id) => published.push(id), (id) => failed.push(id), () => true)
  expect(peak).toBe(3); expect(published).toHaveLength(99); expect(failed).toEqual([11])
})

it('does not publish or enqueue stale query thumbnails', async () => {
  let active = true
  const requested: number[] = []; const published: number[] = []
  await loadProgressiveThumbnails([1, 2, 3, 4, 5], async (id) => {
    requested.push(id); await Promise.resolve(); active = false; return 'cache'
  }, (id) => published.push(id), () => {}, () => active, 2)
  expect(requested).toEqual([1, 2]); expect(published).toEqual([])
})
