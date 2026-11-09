import { describe, expect, it } from 'vitest'
import { calculateDisplayHistogram, calculateHistogram } from './previewPresentation'

describe('display RGB histogram', () => {
  it('matches the previous sampled callback counts exactly at each preview tier', () => {
    for (const width of [512, 1024, 2048]) {
      const height = Math.floor(width * 2 / 3)
      const data = new Uint8ClampedArray(width * height * 4)
      for (let i = 0; i < data.length; i++) data[i] = (i * 67 + Math.floor(i / 17)) % 256
      const pixels = { data, width, height, colorSpace: 'srgb' as const }
      for (const bins of [4, 48, 128]) {
        const channels = [0, 1, 2].map(() => Array.from({ length: bins }, () => 0))
        const stride = Math.max(4, Math.floor(data.length / 300_000 / 4) * 4)
        for (let i = 0; i < data.length; i += stride) {
          if (data[i + 3] === 0) continue
          channels.forEach((channel, component) => {
            channel[Math.min(bins - 1, Math.floor(data[i + component] * bins / 256))]++
          })
        }
        const maximum = Math.max(1, ...channels.flat())
        const [red, green, blue] = channels.map((channel) => channel.map((count) => count / maximum))
        expect(calculateDisplayHistogram(pixels, bins))
          .toEqual({ red, green, blue, luminance: calculateHistogram(pixels, bins) })
      }
    }
  })
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
