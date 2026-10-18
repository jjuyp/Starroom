import { describe, expect, it, vi } from 'vitest'
import type { NativePreviewResult } from './nativeRender'
import { nativePreviewDisplayHistogram } from './nativePreviewPresentation'
import { calculateDisplayHistogram } from './previewPresentation'

const preview = (format: 'rgba8' | 'jpeg', bytes: Uint8Array): NativePreviewResult => ({
  width: 2, height: 1, sourceWidth: 6000, sourceHeight: 4000, tileX: 0, tileY: 0,
  isTile: false, tileOptimized: false, acceleration: 'gpu', inputProfile: 'assumed sRGB',
  cameraProfileId: null, payload: { format, bytes },
})

describe('Native display histogram ownership', () => {
  it('reads only the exact offset RGBA payload without canvas readback or source mutation', () => {
    const backing = new Uint8Array([99, 98, 97, 0, 128, 255, 255, 255, 2, 17, 255, 96])
    const bytes = backing.subarray(3, 11)
    const original = backing.slice()
    const readback = vi.fn(() => { throw new Error('Interactive canvas must not read back') })
    const expected = calculateDisplayHistogram({ data: new Uint8ClampedArray(bytes), width: 2,
      height: 1, colorSpace: 'srgb' })
    expect(nativePreviewDisplayHistogram(preview('rgba8', bytes), readback)).toEqual(expected)
    expect(readback).not.toHaveBeenCalled()
    expect(backing).toEqual(original)
  })

  it('retains the real decoded canvas path for JPEG, including explicit readback failures', () => {
    const pixels = { data: new Uint8ClampedArray([255, 0, 0, 255, 0, 255, 0, 255]),
      width: 2, height: 1, colorSpace: 'srgb' as const }
    const readback = vi.fn(() => pixels)
    expect(nativePreviewDisplayHistogram(preview('jpeg', new Uint8Array()), readback))
      .toEqual(calculateDisplayHistogram(pixels))
    expect(readback).toHaveBeenCalledOnce()
    expect(() => nativePreviewDisplayHistogram(preview('jpeg', new Uint8Array()), () => {
      throw new Error('Canvas readback failed')
    })).toThrow('Canvas readback failed')
  })
})
