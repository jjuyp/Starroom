import { describe, expect, it, vi } from 'vitest'
import { adjustmentKeys, HistoryCommandQueue, HistoryGestureBoundary, nativeHistoryStateChanged } from './historyCommandQueue'

describe('History physical gesture boundary', () => {
  it('does not turn focus, presets or unrelated clicks into a persistence hold', () => {
    const boundary = new HistoryGestureBoundary()
    boundary.begin()
    expect(boundary.commitDelay).toBe(220)
    boundary.inputDown('pointer:7')
    expect(boundary.commitDelay).toBe(220)
    boundary.inputUp('pointer:7')
    expect(boundary.holding).toBe(false)
  })

  it('holds a paused slider for any duration and releases on the matching pointer', () => {
    const boundary = new HistoryGestureBoundary()
    boundary.inputDown('pointer:7')
    boundary.begin()
    expect(boundary.commitDelay).toBeNull()
    boundary.inputUp('pointer:8')
    expect(boundary.commitDelay).toBeNull()
    boundary.inputUp('pointer:7')
    expect(boundary.commitDelay).toBe(220)
  })

  it('coalesces 1000 changes and a long paused hold to one queued semantic command', async () => {
    vi.useFakeTimers()
    try {
      const boundary = new HistoryGestureBoundary()
      const commands = new HistoryCommandQueue()
      let acknowledged = 0
      let pending = 0
      const history: number[] = []
      const schedule = () => {
        const delay = boundary.commitDelay
        if (delay !== null) setTimeout(() => {
          void commands.run(async () => { history.push(acknowledged); acknowledged = pending })
        }, delay)
      }
      boundary.inputDown('pointer:1')
      boundary.begin()
      for (let value = 1; value <= 1000; value++) { pending = value; schedule() }
      await vi.advanceTimersByTimeAsync(10000)
      expect(history).toEqual([])
      boundary.inputUp('pointer:1')
      schedule()
      await vi.advanceTimersByTimeAsync(220)
      await commands.idle()
      expect(history).toEqual([0])
      expect(acknowledged).toBe(1000)
      await commands.run(async () => { acknowledged = history.pop()! })
      expect(acknowledged).toBe(0)
    } finally { vi.useRealTimers() }
  })

  it('keeps repeated keyboard input held until key-up and preserves overlapping input', () => {
    const boundary = new HistoryGestureBoundary()
    boundary.inputDown('key:ArrowRight')
    boundary.begin()
    for (let repeat = 0; repeat < 100; repeat++) boundary.inputDown('key:ArrowRight')
    boundary.inputDown('pointer:2')
    boundary.begin()
    boundary.inputUp('key:ArrowRight')
    expect(boundary.holding).toBe(true)
    boundary.inputUp('pointer:2')
    expect(boundary.holding).toBe(false)
    expect(adjustmentKeys.has('ArrowRight')).toBe(true)
    expect(adjustmentKeys.has('Enter')).toBe(false)
  })

  it('releases cancelled or lost input without permanently blocking future edits', () => {
    const boundary = new HistoryGestureBoundary()
    boundary.inputDown('pointer:1')
    boundary.begin()
    boundary.clear()
    expect(boundary.commitDelay).toBe(220)
    boundary.begin()
    expect(boundary.holding).toBe(false)
    boundary.inputDown('pointer:2')
    boundary.begin()
    boundary.inputUp('pointer:2')
    expect(boundary.holding).toBe(false)
  })

  it('does not attach an unrelated held key to a new pointer editing gesture', () => {
    const boundary = new HistoryGestureBoundary()
    boundary.inputDown('key:ArrowDown') // Library navigation, not an editing begin.
    boundary.inputDown('pointer:2')
    boundary.begin()
    boundary.inputUp('pointer:2')
    expect(boundary.holding).toBe(false)
    boundary.inputUp('key:ArrowDown')
    boundary.begin() // Preset after input was released.
    expect(boundary.holding).toBe(false)
  })
})

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
  it('never commits a phantom edit while hydrating legacy nested LensIdentity aliases', () => {
    const acknowledged: Record<string, unknown> = { optics: { matchMode: 'manual', manualIdentity: {
      camera_make: 'Nikon', camera_model: 'Nikon D750', lens_make: 'Nikon', lens_model: '16-35mm',
      focal_length_mm: 24, aperture: 5.6, focus_distance_m: null,
    } } }
    const current: Record<string, unknown> = { optics: { matchMode: 'manual', manualIdentity: {
      cameraMake: 'Nikon', cameraModel: 'Nikon D750', lensMake: 'Nikon', lensModel: '16-35mm',
      focalLengthMm: 24, aperture: 5.6, focusDistanceM: null,
    } } }
    const originalJson = JSON.stringify(acknowledged)
    expect(nativeHistoryStateChanged(acknowledged, current, false)).toBe(false)
    expect(nativeHistoryStateChanged(acknowledged, { ...current, exposure: 1 }, false)).toBe(true)
    expect(JSON.stringify(acknowledged)).toBe(originalJson)
  })
})
