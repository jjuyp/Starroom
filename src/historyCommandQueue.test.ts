import { describe, expect, it } from 'vitest'
import { HistoryCommandQueue, nativeHistoryStateChanged } from './historyCommandQueue'

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
  it('does not persist focus-only state but saves a later numeric commit and supports undo/redo', async () => {
    const queue = new HistoryCommandQueue()
    let acknowledged = { exposure: 0, layers: [{ tone: { exposure_ev: 0 } }] }
    const previous = structuredClone(acknowledged)
    // Focus can outlive any debounce. There is no timer/gesture token to consume here.
    expect(nativeHistoryStateChanged(acknowledged, structuredClone(acknowledged), false)).toBe(false)
    const committed = { exposure: 0, layers: [{ tone: { exposure_ev: 1 } }] }
    expect(nativeHistoryStateChanged(acknowledged, committed, false)).toBe(true)
    await queue.run(async () => { acknowledged = structuredClone(committed) })
    expect(nativeHistoryStateChanged(acknowledged, committed, false)).toBe(false)
    await queue.run(async () => { acknowledged = structuredClone(previous) })
    expect(acknowledged.layers[0].tone.exposure_ev).toBe(0)
    await queue.run(async () => { acknowledged = structuredClone(committed) })
    expect(acknowledged.layers[0].tone.exposure_ev).toBe(1)
  })
  it('persists a slider continuation after an earlier paused-drag commit', () => {
    const acknowledged = { shadows: 49, blacks: 0 }
    expect(nativeHistoryStateChanged(acknowledged, { shadows: 49, blacks: 30 }, false)).toBe(true)
    expect(nativeHistoryStateChanged(acknowledged, { shadows: 49, blacks: 0 }, false)).toBe(false)
  })
  it('never creates history before open or while restoring acknowledged native state', () => {
    expect(nativeHistoryStateChanged(undefined, { rotation: 90 }, false)).toBe(false)
    expect(nativeHistoryStateChanged({ rotation: 0 }, { rotation: 90 }, true)).toBe(false)
  })
  it('treats sorted Rust JSON and UI insertion order as the same acknowledged state', () => {
    const native = { curve: [{ x: 0, y: 0 }, { x: 1, y: 1 }], exposure: 0, layer: { opacity: 1, tone: { exposure_ev: 1, shadows: 0 } } }
    const ui = { layer: { tone: { shadows: 0, exposure_ev: 1 }, opacity: 1 }, exposure: 0, curve: [{ y: 0, x: 0 }, { y: 1, x: 1 }] }
    expect(nativeHistoryStateChanged(native, ui, false)).toBe(false)
    expect(nativeHistoryStateChanged(native, { ...ui, curve: [...ui.curve].reverse() }, false)).toBe(true)
  })
  it('does not create a phantom edit for omitted JSON optional fields', () => {
    const native: Record<string, unknown> = { layers: [], exposure: 0 }
    const ui: Record<string, unknown> = { exposure: 0, futureOptional: undefined, layers: [] }
    expect(nativeHistoryStateChanged(native, ui, false)).toBe(false)
    expect(nativeHistoryStateChanged(native, { ...ui, exposure: 0.1 }, false)).toBe(true)
  })
})
