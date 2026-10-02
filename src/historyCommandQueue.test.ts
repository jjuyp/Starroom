import { describe, expect, it } from 'vitest'
import { HistoryCommandQueue } from './historyCommandQueue'

describe('Native history command ordering', () => {
  it('acknowledges the last commit before another edit or undo', async () => {
    const queue = new HistoryCommandQueue()
    let state = 0
    const order: number[] = []
    const tasks = [1, 2, 3].map((next) => queue.run(async () => {
      const before = state
      await Promise.resolve()
      expect(state).toBe(before)
      state = next
      order.push(state)
    }))
    await queue.run(async () => { state -= 1; order.push(state) })
    await Promise.all(tasks)
    expect(order).toEqual([1, 2, 3, 2])
  })
  it('continues after a failed command without losing following commands', async () => {
    const queue = new HistoryCommandQueue()
    await expect(queue.run(async () => { throw new Error('disk error') })).rejects.toThrow('disk error')
    await expect(queue.run(async () => 'recovered')).resolves.toBe('recovered')
  })
})
