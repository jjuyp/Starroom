import { describe, expect, it } from 'vitest'
import { calculateHistogram, hasAdjustments, mapToneCurve, processImageData } from './imagePipeline'
import { defaultAdjustments } from './editorState'

function pixels(values: number[]) {
  return { data: new Uint8ClampedArray(values), width: values.length / 4, height: 1 } as ImageData
}

describe('image pipeline', () => {
  it('keeps neutral adjustments pixel-exact', () => {
    const source = pixels([40, 80, 120, 255])
    expect(Array.from(processImageData(source, defaultAdjustments).data)).toEqual([40, 80, 120, 255])
  })

  it('changes rendered pixels when exposure changes', () => {
    const source = pixels([40, 80, 120, 255])
    const result = processImageData(source, { ...defaultAdjustments, exposure: 1 })
    expect(result.data[0]).toBeGreaterThan(40)
    expect(result.data[1]).toBeGreaterThan(80)
    expect(result.data[2]).toBeGreaterThan(120)
  })

  it('treats relative temperature zero as neutral', () => {
    expect(hasAdjustments(defaultAdjustments)).toBe(false)
    expect(hasAdjustments({ ...defaultAdjustments, temperature: 20 })).toBe(true)
  })

  it('applies tone-curve controls to output pixels', () => {
    const source = pixels([35, 35, 35, 255, 128, 128, 128, 255, 220, 220, 220, 255])
    const result = processImageData(source, defaultAdjustments, [
      { id: 'black', x: 0, y: 0 },
      { id: 'shadow', x: .25, y: .5 },
      { id: 'highlight', x: .75, y: .5 },
      { id: 'white', x: 1, y: 1 },
    ])
    expect(result.data[0]).toBeGreaterThan(35)
    expect(result.data[8]).toBeLessThan(220)
  })

  it('keeps monotone curve output monotone for monotone control points', () => {
    const points = [
      { id: 'black', x: 0, y: 0 },
      { id: 'shadow', x: .25, y: .18 },
      { id: 'midtone', x: .5, y: .58 },
      { id: 'white', x: 1, y: 1 },
    ]
    let previous = mapToneCurve(0, points)
    for (let index = 1; index <= 100; index += 1) {
      const output = mapToneCurve(index / 100, points)
      expect(output).toBeGreaterThanOrEqual(previous - 1e-5)
      expect(output).toBeGreaterThanOrEqual(0)
      expect(output).toBeLessThanOrEqual(1)
      previous = output
    }
  })

  it('lifts dark shadows much more than midtones instead of adding a white veil', () => {
    const source = pixels([
      35, 30, 25, 255,
      130, 120, 110, 255,
    ])
    const result = processImageData(source, { ...defaultAdjustments, shadows: 50 })
    const darkGain = result.data[0] - 35
    const midGain = result.data[4] - 130
    expect(darkGain).toBeGreaterThan(0)
    expect(darkGain).toBeGreaterThan(midGain * 2)
  })

  it('keeps the black anchor when shadows are raised', () => {
    const source = pixels([0, 0, 0, 255])
    const result = processImageData(source, { ...defaultAdjustments, shadows: 100 })
    expect(Array.from(result.data)).toEqual([0, 0, 0, 255])
  })

  it('applies a visible sharpness change around an edge', () => {
    const values: number[] = []
    for (let index = 0; index < 9; index += 1) {
      const level = index === 4 ? 160 : 80
      values.push(level, level, level, 255)
    }
    const source = { data: new Uint8ClampedArray(values), width: 3, height: 3 } as ImageData
    const result = processImageData(source, { ...defaultAdjustments, sharpness: 100 })
    expect(result.data[16]).toBeGreaterThan(160)
  })

  it('uses relative temperature values for encoded-image preview', () => {
    const source = pixels([100, 100, 100, 255])
    const warm = processImageData(source, { ...defaultAdjustments, temperature: 70 })
    expect(warm.data[0]).toBeGreaterThan(warm.data[2])
  })

  it('returns a normalized histogram', () => {
    const result = calculateHistogram(pixels([0, 0, 0, 255, 255, 255, 255, 255]), 8)
    expect(Math.max(...result)).toBe(1)
    expect(result[0]).toBeGreaterThan(0)
    expect(result[7]).toBeGreaterThan(0)
  })
})
