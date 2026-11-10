import { describe, expect, it } from 'vitest'
import { nativeCurvePreviewContract, nativeCurveSampleCount, parseNativeCurvePreview } from './nativeRender'
import { LatestPreviewQueue, PreviewSuperseded } from './latestPreviewQueue'

const samples = Array.from({ length: nativeCurveSampleCount }, (_, index) => ({ x: index / 128, y: index / 128 }))

describe('Native curve presentation contract', () => {
  it('sends only native coordinates and bounded sample count, never point IDs or image pixels', () => {
    const points = [{ id: 'black', x: 0, y: .1 }, { id: 'white', x: 1, y: 1.6 }]
    const contract = nativeCurvePreviewContract(points)
    points[0].y = .3
    expect(contract).toEqual({ points: [{ x: 0, y: .1 }, { x: 1, y: 1.6 }], sampleCount: 129 })
    expect(() => nativeCurvePreviewContract([{ x: NaN, y: 0 }])).toThrow('InvalidCurvePreview')
    expect(() => nativeCurvePreviewContract(Array.from({ length: 4097 }, () => ({ x: 0, y: 0 })))).toThrow('InvalidCurvePreview')
  })
  it('preserves finite HDR and negative output samples rather than clamping or interpolating', () => {
    const result = samples.map((sample) => ({ ...sample, y: sample.x * 2 - .1 }))
    expect(parseNativeCurvePreview(result)).toEqual(result)
    expect(parseNativeCurvePreview(result)).not.toBe(result)
  })
  it('rejects malformed, unordered, incomplete or non-finite native geometry', () => {
    for (const bad of [null, samples.slice(1), [...samples, samples[0]],
      samples.map((point, index) => index === 64 ? { ...point, y: Infinity } : point),
      samples.map((point, index) => index === 64 ? { ...point, x: 0 } : point),
      samples.map((point, index) => index === 0 ? { ...point, x: .001 } : point)]) {
      expect(() => parseNativeCurvePreview(bad)).toThrow('InvalidCurvePreview')
    }
  })
  it('keeps native sampling bounded and rejects stale drag/channel replies', async () => {
    const queue = new LatestPreviewQueue<typeof samples>()
    let finish!: (value: typeof samples) => void
    const old = queue.submit(() => new Promise((resolve) => { finish = resolve }), () => undefined).catch((error) => error)
    const middle = queue.submit(async () => samples, () => undefined).catch((error) => error)
    const latest = queue.submit(async () => samples.map((point) => ({ ...point, y: 1 - point.y })), () => undefined)
    finish(samples)
    expect(await old).toBeInstanceOf(PreviewSuperseded)
    expect(await middle).toBeInstanceOf(PreviewSuperseded)
    expect((await latest)[0].y).toBe(1)
  })
})
