import { describe, expect, it } from 'vitest'
import { PreviewLatencyTracker } from './previewLatency'

describe('local edit-to-draw diagnostics', () => {
  it('separates scheduling, queue, Native round trip and presentation without hiding seconds', () => {
    const tracker = new PreviewLatencyTracker(), photo = {}, surface = {}
    tracker.mark(photo, 10)
    const sample = tracker.drawn(photo, surface, 'interactive', { queuedAt: 20, dispatchedAt: 40, receivedAt: 5040 }, 5050)!
    expect(sample).toEqual({ sequence: 1, phase: 'interactive', editToDrawMs: 5040,
      schedulingMs: 10, queueMs: 20, nativeRoundTripMs: 5000, decodeAndDrawMs: 10 })
    tracker.paintOpportunity(sample.sequence, sample.phase, 5055)
    expect(tracker.snapshot()[0].paintOpportunityMs).toBe(5055)
    expect(tracker.drawn(photo, surface, 'interactive', { queuedAt: 5060, dispatchedAt: 5061, receivedAt: 5070 }, 5080)).toBeNull()
    expect(tracker.drawn(photo, surface, 'final', { queuedAt: 5060, dispatchedAt: 5061, receivedAt: 5070 }, 5080)?.editToDrawMs).toBe(5070)
  })

  it('does not treat initial load, invalid clocks or superseded edits as measured updates', () => {
    const tracker = new PreviewLatencyTracker(), first = {}, latest = {}, surface = {}
    const timing = { queuedAt: 20, dispatchedAt: 30, receivedAt: 40 }
    expect(tracker.drawn(first, surface, 'interactive', timing, 50)).toBeNull()
    tracker.mark(first, 1); tracker.mark(latest, 2)
    expect(tracker.drawn(latest, surface, 'interactive', timing, NaN)).toBeNull()
    expect(tracker.drawn(latest, surface, 'interactive', timing, 39)).toBeNull()
    expect(tracker.drawn(latest, surface, 'interactive', timing, 50)).not.toBeNull()
    expect(tracker.drawn(first, surface, 'interactive', timing, 60)).toBeNull()
    const anotherSurface = tracker.drawn(first, {}, 'interactive', timing, 60)!
    expect(anotherSurface.sequence).not.toBe(tracker.snapshot()[0].sequence)
    tracker.paintOpportunity(anotherSurface.sequence, 'interactive', 70)
    expect(tracker.snapshot()[0].paintOpportunityMs).toBeUndefined()
    expect(tracker.snapshot()[1].paintOpportunityMs).toBe(70)
    const reopened = {}
    tracker.mount(reopened, 61)
    expect(tracker.drawn(first, reopened, 'interactive', { queuedAt: 62, dispatchedAt: 63, receivedAt: 64 }, 65)).toBeNull()
  })

  it('bounds retention and exports only detached numeric measurements', () => {
    const tracker = new PreviewLatencyTracker(), surface = {}
    for (let i = 0; i < 400; i++) {
      const photo = { privatePath: 'not-recorded', settings: [1, 2] }
      tracker.mark(photo, 0)
      tracker.drawn(photo, surface, 'interactive', { queuedAt: 1, dispatchedAt: 2, receivedAt: 3 }, 4)
    }
    const records = tracker.snapshot()
    expect(records).toHaveLength(256)
    expect(JSON.stringify(records)).not.toMatch(/privatePath|not-recorded|settings/)
    records[0].editToDrawMs = -1
    expect(tracker.snapshot()[0].editToDrawMs).toBe(4)
  })
})
