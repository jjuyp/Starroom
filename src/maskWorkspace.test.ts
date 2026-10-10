import { describe, expect, it } from 'vitest'
import { generatedMaskActions, hasNativeSkinSelection, invertedMask, replaceRadialGeometry, selectedRadialMask, validLinearGeometry } from './maskWorkspace'
import { defaultAdjustments } from './editorState'
import { defaultNativeSkinRetouch, toNativeSettings, type NativeAdjustmentLayer } from './nativeRender'

describe('selected mask canvas contract', () => {
  it('routes whole-person selection through the scene model rather than face detection', () => {
    expect(generatedMaskActions.find((action) => action.label === '人物')).toEqual({ semantic: 'person', label: '人物', availability: 'sky' })
    expect(generatedMaskActions.map((action) => action.semantic)).toEqual(['subject', 'background', 'person', 'sky'])
  })
  const layer: NativeAdjustmentLayer = { id: 'local', name: '局部', enabled: true, opacity: .7, blendMode: 'normal',
    adjustments: { tone: { exposureEv: 1, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } },
    mask: { type: 'radial', x: .5, y: .5, width: .4, height: .3, rotation: 0, feather: .2, invert: true } }
  it('canvas geometry reaches the native layer without changing legacy mask, feather or inversion', () => {
    const geometry = { x: .3, y: .6, width: .6, height: .4, rotation: 25 }
    const changed = replaceRadialGeometry(layer, geometry)
    const original = { x: .5, y: .5, width: .42, height: .42, rotation: 0 }
    const settings = toNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, [], 'sourceDefault', null, undefined, undefined, [changed], original)
    expect(settings.layers[0].mask).toEqual({ type: 'radial', ...geometry, feather: .2, invert: true })
    expect(settings.layers[1].mask).toMatchObject(original)
    expect(layer.mask).toMatchObject({ x: .5, width: .4 })
  })
  it('non-radial selection cannot accidentally redirect canvas edits to another mask', () => {
    const brush = { ...layer, mask: { type: 'none' as const } }
    expect(selectedRadialMask(brush)).toBeNull()
    expect(replaceRadialGeometry(brush, { x: 0, y: 0, width: 1, height: 1, rotation: 0 })).toBe(brush)
  })
  it('inverting a semantic group round-trips without flattening or losing model identity', () => {
    const group = { operation: 'add' as const, children: [layer.mask] }
    expect(invertedMask(invertedMask(group))).toEqual(group)
  })
  it('linear geometry rejects coincident, nonfinite and out-of-frame handles before native submission', () => {
    const linear = { type: 'linear' as const, startX: .2, startY: .3, endX: .8, endY: .7, feather: .5, invert: false }
    expect(validLinearGeometry(linear)).toBe(true)
    expect(validLinearGeometry({ ...linear, endX: .2, endY: .3 })).toBe(false)
    expect(validLinearGeometry({ ...linear, endX: NaN })).toBe(false)
    expect(validLinearGeometry({ ...linear, startX: -1 })).toBe(false)
  })
  it('only enables AI skin protection when the shared graph references an actual Skin raster', () => {
    const skin = defaultNativeSkinRetouch()
    expect(hasNativeSkinSelection(skin, [layer])).toBe(false)
    expect(hasNativeSkinSelection({ ...skin, faces: [{ faceId: 'face', cacheKey: 'real-cache' }] }, [])).toBe(true)
    const semantic = { ...layer, mask: { type: 'portraitSemantic' as const, faceId: 'face', region: 'skin' as const,
      cacheKey: 'real-cache', modelId: 'bisenet', modelVersion: 'local', modelHash: 'fixture-hash', threshold: .5, feather: .1 } }
    expect(hasNativeSkinSelection(skin, [semantic])).toBe(true)
    expect(hasNativeSkinSelection(skin, [{ ...semantic, enabled: false }])).toBe(false)
    expect(hasNativeSkinSelection(skin, [{ ...semantic, mask: { operation: 'invert', children: [semantic.mask] } }])).toBe(true)
  })
})
