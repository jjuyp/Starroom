import { describe, expect, it } from 'vitest'
import { assertNativeSupported, parseNativePreviewFrame, toNativeSettings } from './nativeRender'
import { defaultAdjustments } from './editorState'

const defaultMask = { x: .5, y: .5, width: .42, height: .42, rotation: 0 }

describe('native preview contract', () => {
  it('parses the versioned binary frame without JSON pixel arrays', () => {
    const payload = new Uint8Array([0xff, 0xd8, 0xff])
    const profile = new TextEncoder().encode('dng-forward-matrix:test:camera')
    const frame = new Uint8Array(24 + profile.length + payload.length)
    frame.set([83, 82, 80, 50])
    const view = new DataView(frame.buffer)
    view.setUint16(4, 2, true)
    view.setUint16(6, 2, true)
    view.setUint32(8, 640, true)
    view.setUint32(12, 480, true)
    view.setUint16(16, profile.length, true)
    view.setUint32(20, payload.length, true)
    frame.set(profile, 24)
    frame.set(payload, 24 + profile.length)
    expect(parseNativePreviewFrame(frame)).toEqual({
      width: 640,
      height: 480,
      acceleration: 'cpuFallback',
      inputProfile: 'resolved RAW camera profile',
      cameraProfileId: 'dng-forward-matrix:test:camera',
      jpeg: payload,
    })
  })

  it('serializes exposure, WB, tone and curve without creative image math in TypeScript', () => {
    const settings = toNativeSettings(
      { ...defaultAdjustments, exposure: .75, temperature: 25, shadows: 30 },
      [{ id: 'white', x: 1, y: 1 }, { id: 'black', x: 0, y: 0 }],
    )
    expect(settings.exposure).toBe(.75)
    expect(settings.temperature).toBe(25)
    expect(settings.shadows).toBe(30)
    expect(settings.whiteBalanceMode).toBe('sourceDefault')
    expect(settings.whiteBalanceSample).toBeNull()
    expect(settings.curve).toEqual([{ x: 0, y: 0 }, { x: 1, y: 1 }])
    expect(settings.colorMixer.bands).toHaveLength(8)
  })

  it('transports ordered layer intent without calculating pixels in TypeScript', () => {
    const layers = [{ id: 'lift', name: 'Lift foreground', enabled: true, opacity: .6, blendMode: 'normal' as const,
      mask: { type: 'none' as const },
      adjustments: { tone: { exposureEv: .75, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } } }]
    const settings = toNativeSettings(defaultAdjustments, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, layers)
    expect(settings.layers).toEqual(layers)
  })

  it('serializes the interactive radial mask as a native layer instead of a browser fallback', () => {
    const settings = toNativeSettings({ ...defaultAdjustments, maskExposure: 1, maskFeather: 40 }, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, [], { x: .25, y: .75, width: .3, height: .5, rotation: 18 })
    expect(settings.layers).toHaveLength(1)
    expect(settings.layers[0]).toMatchObject({ id: '__m15-radial-mask__', mask: { type: 'radial', x: .25, y: .75, feather: .4 } })
  })

  it('never silently ignores edits outside the M1C native slice', () => {
    expect(() => assertNativeSupported({ ...defaultAdjustments, lensBrightness: 10 }, defaultMask))
      .toThrow(/Browser fallback was not used/)
  })

  it('transports M21 residual controls and M23 finishing effects without pixels', () => {
    const settings = toNativeSettings({ ...defaultAdjustments, aiDenoiseEnabled: 1, aiDenoiseAmount: 72,
      aiDenoiseDetail: 64, aiDenoiseColorNoise: 38, aiDenoisePreserveSkin: 81,
      grainAmount: 22, grainSize: 45, grainRoughness: 67, grainColor: 28,
      vignette: 30, vignetteHighlightProtect: 74 }, [], 'sourceDefault')
    expect(settings.aiDenoise).toEqual({ enabled: true, amount: .72, detail: .64, colorNoise: .38, preserveSkin: .81 })
    expect(settings).not.toHaveProperty('aiDenoiseResidual')
    expect(settings.grain).toMatchObject({ amount: .22, size: .45, roughness: .67, color: .28 })
    expect(settings.vignette).toMatchObject({ amount: .3, highlightProtect: .74 })
  })

  it('serializes all four curve channels as a compact native contract', () => {
    const identity = [{ id: 'black', x: 0, y: 0 }, { id: 'white', x: 1, y: 1 }]
    const settings = toNativeSettings(defaultAdjustments, identity, 'sourceDefault', null, {
      master: identity,
      red: [{ id: 'r0', x: 0, y: .1 }, { id: 'r1', x: 1, y: 1 }],
      green: [],
      blue: [{ id: 'b0', x: 0, y: 0 }, { id: 'b1', x: 1, y: .9 }],
    })
    expect(settings.curves.master).toEqual([{ x: 0, y: 0 }, { x: 1, y: 1 }])
    expect(settings.curves.red[0]).toEqual({ x: 0, y: .1 })
    expect(settings.curves.blue[1]).toEqual({ x: 1, y: .9 })
  })

  it('serializes eight native OKLCh mixer bands without browser color math', () => {
    const settings = toNativeSettings({
      ...defaultAdjustments, mixerCyanHue: -12, mixerCyanChroma: 45, mixerCyanLightness: -20,
    }, [])
    expect(settings.colorMixer.bands[4]).toEqual({ hueDegrees: -12, chroma: .45, lightness: -.2 })
    expect(settings.colorMixer.hueLock).toBe(true)
  })

  it('serializes all four grading vectors and crossover controls', () => {
    const settings = toNativeSettings({ ...defaultAdjustments, gradeShadowsHue: -140,
      gradeShadowsChroma: 35, gradeHighlightsLightness: -12, gradeBalance: 20, gradeBlending: 75, gradeAmount: 80 }, [])
    expect(settings.grading.shadows).toEqual({ hueDegrees: -140, chroma: .35, lightness: 0 })
    expect(settings.grading.highlights.lightness).toBe(-.12)
    expect(settings.grading).toMatchObject({ balance: .2, blending: .75, amount: .8 })
  })

  it('serializes distinct sharpen denoise and local-detail controls', () => {
    const settings = toNativeSettings({ ...defaultAdjustments, sharpness: 40, sharpenRadius: 1.8,
      sharpenMasking: 65, denoiseLuminance: 30, denoiseChroma: 60, denoiseHighIso: 80,
      texture: 25, clarity: -15, dehaze: 20 }, [])
    expect(settings.sharpenSettings).toMatchObject({ amount: .8, radius: 1.8, masking: .65 })
    expect(settings.denoiseSettings).toMatchObject({ luminance: .3, chroma: .6, highIso: .8 })
    expect(settings.localDetail).toEqual({ texture: .25, clarity: -.15, dehaze: .2 })
  })

  it('serializes Lensfun switches and explicit manual identity', () => {
    const identity = { cameraMake: 'Nikon', cameraModel: 'Nikon D750', lensMake: 'Nikon',
      lensModel: 'Nikon AF-S Nikkor 16-35mm f/4G ED VR', focalLengthMm: 24, aperture: 5.6, focusDistanceM: 10 }
    const settings = toNativeSettings({ ...defaultAdjustments, lensCorrection: 1, lensTca: 0 }, [],
      'sourceDefault', null, { master: [], red: [], green: [], blue: [] }, { matchMode: 'manual', manualIdentity: identity })
    expect(settings.optics.parameters).toMatchObject({ enabled: true, distortion: true, tca: false, vignette: true, autoScale: true })
    expect(settings.optics.manualIdentity).toEqual(identity)
  })

  it('serializes crop perspective upright and four-point geometry without browser image math', () => {
    const settings = toNativeSettings({ ...defaultAdjustments, rotation: 4.25, geometryVertical: 30,
      geometryHorizontal: -20, geometryScale: 112, geometryOffsetX: 8, geometryOffsetY: -5,
      cropLeft: 10, cropTop: 15, cropRight: 90, cropBottom: 85, cropAspectWidth: 3,
      cropAspectHeight: 2, geometryUpright: 4, geometryFourPoint: 1, quadTopLeftX: 8,
      quadTopLeftY: 4, quadTopRightX: 93, quadTopRightY: 9 }, [])
    expect(settings.geometry).toMatchObject({ rotationDegrees: 4.25, verticalKeystone: .3,
      horizontalKeystone: -.2, scale: 1.12, offsetX: .08, offsetY: -.05,
      crop: { left: .1, top: .15, right: .9, bottom: .85 }, cropAspectWidth: 3,
      cropAspectHeight: 2, uprightMode: 'full' })
    expect(settings.geometry.fourPoint?.topLeft).toEqual({ x: .08, y: .04 })
    expect(settings.geometry.fourPoint?.topRight).toEqual({ x: .93, y: .09 })
  })

  it('transports a compact M16 portrait cache reference, never semantic pixels', () => {
    const layers = [{ id: 'portrait-skin', name: 'Portrait skin', enabled: true, opacity: 1, blendMode: 'normal' as const,
      mask: { type: 'portraitSemantic' as const, faceId: 'face-123', region: 'skin' as const, threshold: .55, feather: .08,
        modelId: 'yakhyo/face-parsing-bisenet-resnet18', modelVersion: '8a4729d', modelHash: 'a'.repeat(64), cacheKey: 'face-123:crop' },
      adjustments: { tone: { exposureEv: .3, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } } }]
    const settings = toNativeSettings(defaultAdjustments, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, layers)
    expect(settings.layers[0].mask).toMatchObject({ type: 'portraitSemantic', cacheKey: 'face-123:crop', region: 'skin' })
    expect(JSON.stringify(settings)).not.toContain('values')
  })

  it('serializes M17 skin controls and face cache identities without semantic pixels', () => {
    const settings = toNativeSettings(defaultAdjustments, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, [], defaultMask, {
        parameters: { smooth: .45, texture: .7, toneEvenness: .25, hueDegrees: 8, chroma: -.15, exposureEv: .3 },
        faces: [{ faceId: 'face-123', cacheKey: 'face-123:crop' }],
      })
    expect(settings.skinRetouch).toEqual({
      parameters: { smooth: .45, texture: .7, toneEvenness: .25, hueDegrees: 8, chroma: -.15, exposureEv: .3 },
      faces: [{ faceId: 'face-123', cacheKey: 'face-123:crop' }],
    })
    expect(JSON.stringify(settings.skinRetouch)).not.toContain('values')
  })

  it('serializes M18 healing intent with source coordinates only', () => {
    const settings = toNativeSettings(defaultAdjustments, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, [], defaultMask, undefined, [{
        id: 'heal-1', enabled: true, mode: 'heal', target: { x: .5, y: .4 }, source: null,
        radius: 24, feather: .55, opacity: .85, rotationDegrees: 0, scale: 1,
        toneAdaptation: true, textureAdaptation: true, sourceMode: 'auto', metadata: { interaction: 'brush' },
      }])
    expect(settings.healingOperations[0]).toMatchObject({ id: 'heal-1', sourceMode: 'auto', target: { x: .5, y: .4 } })
    expect(JSON.stringify(settings.healingOperations)).not.toContain('pixels')
  })

  it('serializes M20 GeneratedMaskNode metadata without raster pixels', () => {
    const layers = [{ id: 'ai-sky', name: 'AI sky', enabled: true, opacity: 1, blendMode: 'normal' as const,
      mask: { type: 'generated' as const, providerId: 'semantic-scene', modelId: 'segformer-b0-ade20k-sky',
        modelVersion: '489d5cd81a0b59fab9b7ea758d3548ebe99677da', modelHash: 'd'.repeat(64), semanticClass: 'sky' as const,
        threshold: .5, feather: .08, invert: false, cacheIdentity: 'source-provider-model-params',
        metadata: { executionProvider: 'cpu' } },
      adjustments: { tone: { exposureEv: -.2, contrast: 0, highlights: -10, shadows: 0, whites: 0, blacks: 0 } } }]
    const settings = toNativeSettings(defaultAdjustments, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, layers)
    expect(settings.layers[0].mask).toMatchObject({ type: 'generated', semanticClass: 'sky', cacheIdentity: 'source-provider-model-params' })
    expect(JSON.stringify(settings)).not.toContain('values')
    expect(JSON.stringify(settings)).not.toContain('pixels')
  })
})
