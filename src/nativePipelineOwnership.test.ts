import { describe, expect, it } from 'vitest'
import { calculateHistogram, hasAdjustments } from './previewPresentation'
import { defaultAdjustments } from './editorState'

describe('single Native image engine and UI presentation', () => {
  it('keeps relative temperature zero neutral in UI edit intent', () => {
    expect(hasAdjustments(defaultAdjustments)).toBe(false)
    expect(hasAdjustments({ ...defaultAdjustments, temperature: 20 })).toBe(true)
  })
  it('returns the existing normalized display histogram without altering pixels', () => {
    const data = new Uint8ClampedArray([0, 0, 0, 255, 255, 255, 255, 255])
    const original = data.slice()
    const result = calculateHistogram({ data, width: 2, height: 1 } as ImageData, 8)
    expect(Math.max(...result)).toBe(1)
    expect(result[0]).toBeGreaterThan(0)
    expect(result[7]).toBeGreaterThan(0)
    expect(data).toEqual(original)
  })
})
