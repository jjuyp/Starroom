import { describe, expect, it } from 'vitest'
import { defaultAdjustments } from './editorState'
import { nativeColorSampleContract, toNativeSettings } from './nativeRender'

describe('native post-geometry sampling contract', () => {
  it('transports the complete current graph and explicit coordinate/decode intent', () => {
    const settings = toNativeSettings(defaultAdjustments, [])
    settings.geometry.rotationDegrees = 90
    settings.geometry.flipHorizontal = true
    settings.geometry.crop = { left: .2, top: .1, right: .8, bottom: .9 }
    settings.optics.parameters.enabled = true
    settings.optics.matchMode = 'manual'
    settings.optics.manualIdentity = {
      cameraMake: 'Nikon', cameraModel: 'D750', lensMake: 'Nikon',
      lensModel: 'AF-S Nikkor 24-120mm f/4G ED VR', focalLengthMm: 50, aperture: 4,
      focusDistanceM: null,
    }
    settings.whiteBalanceMode = 'neutralPicker'
    settings.whiteBalanceSample = { x: .3, y: .4, width: .01, height: .01 }
    const request = nativeColorSampleContract('C:/photos/photo.nef', .3, .4, settings, 2048)
    expect(request).toEqual({ sourcePath: 'C:/photos/photo.nef', x: .3, y: .4,
      coordinateSpace: 'postGeometry', maxEdge: 2048, settings })
    expect(request.settings).not.toBe(settings)
    settings.geometry.rotationDegrees = 180
    expect(request.settings.geometry.rotationDegrees).toBe(90)
    expect(JSON.stringify(request)).not.toContain('pixels')
  })

  it('rejects invalid coordinates or decode ranges without invoking a browser fallback', () => {
    const settings = toNativeSettings(defaultAdjustments, [])
    for (const [x, y, edge] of [[NaN, 0, 1800], [0, Infinity, 1800], [-.1, .5, 1800],
      [.5, 1.1, 1800], [.5, .5, 64], [.5, .5, 8192], [.5, .5, 1024.5]]) {
      expect(() => nativeColorSampleContract('photo.nef', x, y, settings, edge))
        .toThrow('NativeColorSampleInvalid')
    }
  })
})
