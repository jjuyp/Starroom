import { describe, expect, it } from 'vitest'
import { countAdditionalEditIntent, type AdditionalEditIntent } from './editIntent'
import { defaultNativeRenderConstants, type NativeRenderConstants, type NativeWhiteBalanceMode } from './nativeRender'

const original = (): AdditionalEditIntent => ({ curveChannels: { master: [], red: [], green: [], blue: [] },
  whiteBalanceMode: 'sourceDefault', whiteBalanceSample: null })

describe('complete workspace edit intent', () => {
  it('does not mark an untouched photo or explicit default graph constants as edited', () => {
    expect(countAdditionalEditIntent(original())).toBe(0)
    expect(countAdditionalEditIntent({ ...original(), nativeRenderConstants: { ...defaultNativeRenderConstants } })).toBe(0)
  })
  it.each(['red', 'green', 'blue'] as const)('marks a pure %s channel curve as edited for Reset/Edited/close state', (channel) => {
    const photo = original()
    photo.curveChannels[channel] = [{ id: 'black', x: 0, y: .05 }, { id: 'white', x: 1, y: 1 }]
    expect(countAdditionalEditIntent(photo)).toBe(1)
  })
  it.each<NativeWhiteBalanceMode>(['asShot', 'camera', 'auto', 'neutralPicker', 'relative'])('marks a standalone %s WB choice as edit intent', (mode) => {
    expect(countAdditionalEditIntent({ ...original(), whiteBalanceMode: mode })).toBe(1)
  })
  it('tracks neutral-picker sample state independently of scalar temperature/tint', () => {
    expect(countAdditionalEditIntent({ ...original(), whiteBalanceSample: { x: .4, y: .5, width: .02, height: .02 } })).toBe(1)
  })
  it.each<[keyof NativeRenderConstants, number | 'cpu']>([
    ['sharpenThreshold', .007], ['mixerBandWidthDegrees', 63], ['grainSeed', 426], ['aiDenoiseProvider', 'cpu'],
  ])('tracks native-authored hidden %s intent without inventing a new UI knob', (key, value) => {
    expect(countAdditionalEditIntent({ ...original(), nativeRenderConstants: { ...defaultNativeRenderConstants, [key]: value } })).toBe(1)
  })
})
