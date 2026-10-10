import { describe, expect, it } from 'vitest'
import { calculateDisplayHistogram } from './previewPresentation'

describe('display RGB histogram', () => {
  it('keeps red, green and blue channels distinct without changing preview pixels', () => {
    const pixels = new Uint8ClampedArray([
      255, 0, 0, 255,
      0, 128, 0, 255,
      0, 0, 255, 255,
      200, 200, 200, 0,
    ])
    const copy = pixels.slice()
    const result = calculateDisplayHistogram({ data: pixels, width: 2, height: 2, colorSpace: 'srgb' }, 4)
    expect(result.red).toEqual([1, 0, 0, 0.5])
    expect(result.green).toEqual([1, 0, 0.5, 0])
    expect(result.blue).toEqual([1, 0, 0, 0.5])
    expect(pixels).toEqual(copy)
  })
})
