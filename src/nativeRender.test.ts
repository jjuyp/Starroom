import { describe, expect, it } from 'vitest'
import { defaultNativeRenderConstants, fromNativeLayers, fromNativeMask, fromNativeSettings, nativePreviewViewportContract, normalizeNativeSettings, parseNativePreviewFrame, toNativeLayers, toNativeMask, toNativeSettings,
  type NativeAdjustmentLayer, type NativeEditSettings, type NativeMaskTree, type NativeSerializedAdjustmentLayer } from './nativeRender'
import { defaultAdjustments } from './editorState'
import localLayersFixture from '../fixtures/contracts/native-local-layers.json'
import localWorkflowFixture from '../fixtures/contracts/native-local-workflow.json'
import canonicalNativeDefaults from '../fixtures/contracts/native-default-settings.json'

const defaultMask = { x: .5, y: .5, width: .42, height: .42, rotation: 0 }

describe('native preview contract', () => {
  it('requests full source resolution for 1:1/high zoom while retaining a bounded Fit tier', () => {
    expect(nativePreviewViewportContract('fit', 1, 6000, 4000)).toEqual({ resolutionMode: 'fit', maxEdge: 1800, viewport: null })
    expect(nativePreviewViewportContract('fit', 1, 6000, 4000, undefined, 900)).toEqual({ resolutionMode: 'fit', maxEdge: 1800, viewport: null })
    expect(nativePreviewViewportContract('fit', 2, 6000, 4000, undefined, 2200)).toEqual({ resolutionMode: 'fit', maxEdge: 2200, viewport: null })
    expect(nativePreviewViewportContract('fit', 5, 6000, 4000, undefined, 4600)).toEqual({ resolutionMode: 'highResolution', maxEdge: 6000, viewport: null })
    expect(nativePreviewViewportContract('100', 1, 6000, 4000, {
      centerX: .5, centerY: .5, widthFraction: .2, heightFraction: .25,
    })).toEqual({ resolutionMode: 'highResolution', maxEdge: 6000, viewport: { sourceWidth: 6000, sourceHeight: 4000, x: 2250, y: 1375, width: 1500, height: 1250 } })
  })
  it('parses the versioned binary frame without JSON pixel arrays', () => {
    const payload = new Uint8Array([0xff, 0xd8, 0xff])
    const profile = new TextEncoder().encode('dng-forward-matrix:test:camera')
    const frame = new Uint8Array(40 + profile.length + payload.length)
    frame.set([83, 82, 80, 51])
    const view = new DataView(frame.buffer)
    view.setUint16(4, 3, true)
    view.setUint16(6, 2 | 0x20 | 0x40, true)
    view.setUint32(8, 640, true)
    view.setUint32(12, 480, true)
    view.setUint32(16, 6000, true)
    view.setUint32(20, 4000, true)
    view.setUint32(24, 512, true)
    view.setUint32(28, 256, true)
    view.setUint16(32, profile.length, true)
    view.setUint32(36, payload.length, true)
    frame.set(profile, 40)
    frame.set(payload, 40 + profile.length)
    expect(parseNativePreviewFrame(frame)).toEqual({
      width: 640,
      height: 480,
      sourceWidth: 6000,
      sourceHeight: 4000,
      tileX: 512,
      tileY: 256,
      isTile: true,
      tileOptimized: true,
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
    expect(settings.layers).toEqual([{ ...layers[0], adjustments: { tone: { exposure_ev: .75,
      contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } } }])
  })

  it('serializes the interactive radial mask as a native layer instead of a browser fallback', () => {
    const settings = toNativeSettings({ ...defaultAdjustments, maskExposure: 1, maskFeather: 40 }, [], 'sourceDefault', null,
      { master: [], red: [], green: [], blue: [] }, undefined, [], { x: .25, y: .75, width: .3, height: .5, rotation: 18 })
    expect(settings.layers).toHaveLength(1)
    expect(settings.layers[0]).toMatchObject({ id: '__m15-radial-mask__', mask: { type: 'radial', x: .25, y: .75, feather: .4 } })
    expect(settings.layers[0].adjustments.tone.exposure_ev).toBe(1)
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
    expect(settings.layers[0].mask).toMatchObject({ type: 'portraitSemantic', cache_key: 'face-123:crop', region: 'skin' })
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
    expect(settings.layers[0].mask).toMatchObject({ type: 'generated', semantic_class: 'sky', cache_identity: 'source-provider-model-params' })
    expect(JSON.stringify(settings)).not.toContain('values')
    expect(JSON.stringify(settings)).not.toContain('pixels')
  })

  it('round-trips the shared Rust local-layer wire fixture through native response, editor state and request', () => {
    // This exact contract fixture is also deserialized by the real Rust preview command test.
    // Its model/cache strings are schema examples, not a claim of model availability.
    const wire = structuredClone(localLayersFixture) as NativeSerializedAdjustmentLayer[]
    const original = JSON.stringify(wire)
    const state = fromNativeSettings(defaultAdjustments, {
      ...toNativeSettings(defaultAdjustments, []), layers: wire,
    })
    expect(state.layers.map((layer) => layer.adjustments.tone.exposureEv)).toEqual(wire.map((layer) => layer.adjustments.tone.exposure_ev))
    expect(state.layers[0].mask).toEqual(fromNativeMask(wire[0].mask))
    const settings = toNativeSettings(state.adjustments, state.curves.master, 'sourceDefault', null,
      state.curves, undefined, state.layers)
    expect(JSON.parse(JSON.stringify(settings)).layers).toEqual(wire)
    for (const layer of settings.layers) {
      expect(layer.adjustments.tone).toHaveProperty('exposure_ev')
      expect(layer.adjustments.tone).not.toHaveProperty('exposureEv')
    }
    state.layers[0].adjustments.tone.exposureEv = 1.25
    const edited = toNativeSettings(state.adjustments, state.curves.master, 'sourceDefault', null,
      state.curves, undefined, state.layers)
    expect(edited.layers[0].adjustments.tone.exposure_ev).toBe(1.25)
    expect(JSON.stringify(wire)).toBe(original)
  })

  it('migrates old camelCase project/history layer intent into canonical native requests without losing values', () => {
    const wire = structuredClone(localLayersFixture) as NativeSerializedAdjustmentLayer[]
    const legacy = fromNativeLayers(wire)
    // Legacy frontend histories used exposureEv. Reopen/undo/snapshot normalization must
    // accept both shapes rather than silently reset local exposure or drop an AI mask.
    const restored = fromNativeLayers(JSON.parse(JSON.stringify(legacy)) as NativeAdjustmentLayer[])
    expect(restored).toEqual(legacy)
    expect(toNativeLayers(restored)).toEqual(wire)
    expect(fromNativeLayers(toNativeLayers(restored))).toEqual(restored)
  })

  it('recursively serializes composite mask leaves with actual Rust field names', () => {
    const layers = fromNativeLayers(structuredClone(localLayersFixture) as NativeSerializedAdjustmentLayer[])
    const request = toNativeLayers(layers)
    const portrait = request[0].mask
    expect('children' in portrait && portrait.children[0]).toMatchObject({ face_id: 'face-contract-1',
      model_id: 'yakhyo/face-parsing-bisenet-resnet18', cache_key: 'portrait-contract:face-crop-1' })
    expect(request[1].mask).toMatchObject({ provider_id: 'birefnet', semantic_class: 'background',
      cache_identity: 'contract-source-provider-model-params' })
    expect(request[2].mask).toEqual({ type: 'linear', start_x: .2, start_y: .1, end_x: .8, end_y: .9, feather: .25, invert: false })
    const encoded = JSON.stringify(request)
    for (const uiKey of ['faceId', 'modelId', 'cacheKey', 'providerId', 'semanticClass', 'cacheIdentity', 'startX', 'endY']) {
      expect(encoded).not.toContain(`"${uiKey}"`)
    }
    const nested: NativeMaskTree = { operation: 'subtract', children: [
      { operation: 'invert', children: [layers[0].mask] }, layers[1].mask, layers[2].mask,
    ] }
    expect(fromNativeMask(JSON.parse(JSON.stringify(toNativeMask(nested))))).toEqual(nested)
  })

  it('preserves native local color/curve extensions while adapting tone and mask names', () => {
    const layer = structuredClone(localLayersFixture[0]) as NativeSerializedAdjustmentLayer
    layer.adjustments.relativeColor = { temperature: .2, tint: -.1, vibrance: .1, saturation: -.2 }
    layer.adjustments.curves = { master: [{ x: 0, y: .03 }, { x: 1, y: 1 }], red: [], green: [], blue: [] }
    const ui = fromNativeLayers([layer])
    expect(toNativeLayers(ui)).toEqual([layer])
    ui[0].adjustments.tone.exposureEv = .8
    expect(toNativeLayers(ui)[0].adjustments.curves).toEqual(layer.adjustments.curves)
    expect(toNativeLayers(ui)[0].adjustments.relativeColor).toEqual(layer.adjustments.relativeColor)
  })

  it('preserves native-returned Skin/Healing and hidden graph values across the next real request', () => {
    const wire = { ...toNativeSettings(defaultAdjustments, []), ...structuredClone(localWorkflowFixture),
      layers: structuredClone(localLayersFixture) } as NativeEditSettings
    const original = JSON.stringify(wire)
    const ui = fromNativeSettings(defaultAdjustments, wire)
    expect(ui.adjustments.sharpness).toBe(45)
    expect(ui.adjustments.noiseReduction).toBe(12)
    const request = toNativeSettings(ui.adjustments, ui.curves.master, ui.settings.whiteBalanceMode,
      ui.settings.whiteBalanceSample, ui.curves, ui.settings.optics, ui.layers, ui.mask ?? defaultMask,
      ui.settings.skinRetouch, ui.settings.healingOperations, ui.renderConstants)
    expect(request.skinRetouch).toEqual(localWorkflowFixture.skinRetouch)
    expect(request.healingOperations).toEqual(localWorkflowFixture.healingOperations)
    expect(request.layers).toEqual(localLayersFixture)
    expect(request.sharpenSettings).toEqual(localWorkflowFixture.sharpenSettings)
    expect(request.denoiseSettings).toEqual(localWorkflowFixture.denoiseSettings)
    expect(request.colorMixer).toEqual(localWorkflowFixture.colorMixer)
    expect(request.grain).toEqual(localWorkflowFixture.grain)
    expect(request.aiDenoiseProvider).toBe('cpu')
    expect(JSON.stringify(wire)).toBe(original)
  })

  it('uses the exact native serde defaults for legal old history, not the different neutral UI defaults', () => {
    const legacy = Object.fromEntries(Object.entries(canonicalNativeDefaults)
      .filter(([key]) => ['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'temperature', 'tint',
        'vibrance', 'saturation', 'sharpness', 'noiseReduction', 'curve'].includes(key))) as unknown as NativeEditSettings
    const original = JSON.stringify(legacy)
    expect(normalizeNativeSettings(legacy)).toEqual(canonicalNativeDefaults)
    const ui = fromNativeSettings(defaultAdjustments, legacy)
    expect(ui.adjustments.sharpness).toBe(17.5)
    expect(ui.settings.sharpenSettings.amount).toBe(.35)
    expect(ui.settings.optics).toEqual(canonicalNativeDefaults.optics)
    expect(ui.settings.skinRetouch).toEqual(canonicalNativeDefaults.skinRetouch)
    expect(ui.settings.vignette).toEqual(canonicalNativeDefaults.vignette)
    expect(JSON.stringify(legacy)).toBe(original)
  })

  it('keeps the legal legacy S-curve when modern master/channel families were absent or underspecified', () => {
    const curve = [{ x: 0, y: 0 }, { x: .25, y: .18 }, { x: .75, y: .83 }, { x: 1, y: 1 }]
    const legacy = { ...structuredClone(canonicalNativeDefaults), curve, curves: { red: [] } } as unknown as NativeEditSettings
    const original = JSON.stringify(legacy)
    const ui = fromNativeSettings(defaultAdjustments, legacy)
    expect(ui.curves.master.map(({ x, y }) => ({ x, y }))).toEqual(curve)
    const request = toNativeSettings(ui.adjustments, ui.curves.master, ui.settings.whiteBalanceMode,
      ui.settings.whiteBalanceSample, ui.curves, ui.settings.optics, ui.layers, defaultMask,
      ui.settings.skinRetouch, ui.settings.healingOperations, ui.renderConstants)
    expect(request.curves.master).toEqual(curve)
    expect(request.curve).toEqual(curve)
    expect(JSON.stringify(legacy)).toBe(original)
  })

  it('hydrates legal id/name-only layers and empty local adjustment groups using native defaults', () => {
    const layers = [{ id: 'legacy-none', name: 'Old whole-image layer' },
      { id: 'legacy-empty', name: 'Old adjustments', adjustments: {} }] as unknown as NativeSerializedAdjustmentLayer[]
    const original = JSON.stringify(layers)
    const ui = fromNativeLayers(layers)
    for (const layer of ui) {
      expect(layer).toMatchObject({ enabled: true, opacity: 1, blendMode: 'normal', mask: { type: 'none' },
        adjustments: { tone: { exposureEv: 0, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } } })
    }
    expect(toNativeLayers(ui)[0].adjustments.tone.exposure_ev).toBe(0)
    expect(JSON.stringify(layers)).toBe(original)
  })

  it('rejects unsafe native u64 seeds rather than silently changing deterministic grain texture', () => {
    const settings = toNativeSettings(defaultAdjustments, [])
    settings.grain.seed = Number.MAX_SAFE_INTEGER + 1
    expect(() => fromNativeSettings(defaultAdjustments, settings)).toThrow('NativeRenderContractInvalid')
    expect(() => toNativeSettings(defaultAdjustments, [], undefined, undefined, undefined, undefined, undefined,
      undefined, undefined, undefined, { ...defaultNativeRenderConstants, grainSeed: Number.MAX_SAFE_INTEGER + 1 }))
      .toThrow('NativeRenderContractInvalid')
  })

  it('restores legacy radial exposure and geometry exactly once through history/snapshot hydration', () => {
    const geometry = { x: .25, y: .75, width: .3, height: .5, rotation: 18 }
    const baseline = { ...defaultAdjustments, maskExposure: 1.2, maskFeather: 35 }
    const wire = toNativeSettings(baseline, [], 'sourceDefault', null, undefined, undefined, [], geometry)
    const restored = fromNativeSettings({ ...defaultAdjustments, maskExposure: .8 }, JSON.parse(JSON.stringify(wire)))
    expect(restored.layers).toHaveLength(0)
    expect(restored.adjustments.maskExposure).toBe(1.2)
    expect(restored.adjustments.maskFeather).toBe(35)
    expect(restored.mask).toEqual(geometry)
    const roundTrip = toNativeSettings(restored.adjustments, restored.curves.master, 'sourceDefault', null,
      restored.curves, undefined, restored.layers, restored.mask!)
    expect(roundTrip.layers).toEqual(wire.layers)
    expect(roundTrip.layers).toHaveLength(1)
    expect(fromNativeSettings(restored.adjustments, toNativeSettings(defaultAdjustments, [])).adjustments.maskExposure).toBe(0)
  })

  it('restores a native-serialized reserved radial with full neutral layer defaults as a single center control', () => {
    const wire = toNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, [])
    Object.assign(wire.layers[0].adjustments, {
      relativeColor: { temperature: 0, tint: 0, vibrance: 0, saturation: 0 },
      curves: { master: [], red: [], green: [], blue: [] },
      colorMixer: canonicalNativeDefaults.colorMixer, grading: canonicalNativeDefaults.grading,
    })
    const ui = fromNativeSettings(defaultAdjustments, wire)
    expect(ui.layers).toHaveLength(0)
    expect(ui.adjustments.maskExposure).toBe(1)
    const changed = toNativeSettings({ ...ui.adjustments, maskExposure: 1.5 }, [], 'sourceDefault', null,
      undefined, undefined, ui.layers, ui.mask!)
    expect(changed.layers).toHaveLength(1)
    expect(changed.layers[0].id).toBe('__m15-radial-mask__')
    expect(changed.layers[0].adjustments.tone.exposure_ev).toBe(1.5)
  })

  it('retains nonneutral reserved local intent and allocates a distinct center-control identity', () => {
    const wire = toNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, [])
    wire.layers[0].adjustments.relativeColor = { temperature: .2, tint: -.1, vibrance: .1, saturation: .1 }
    const ui = fromNativeSettings(defaultAdjustments, wire)
    expect(ui.layers).toHaveLength(1)
    expect(ui.adjustments.maskExposure).toBe(0)
    const changed = toNativeSettings({ ...ui.adjustments, maskExposure: .5 }, [], 'sourceDefault', null,
      undefined, undefined, ui.layers, defaultMask)
    expect(changed.layers).toHaveLength(2)
    expect(new Set(changed.layers.map(({ id }) => id)).size).toBe(2)
    expect(changed.layers[0]).toEqual(wire.layers[0])
    const hydrated = fromNativeSettings(ui.adjustments, changed)
    expect(hydrated.layers).toHaveLength(1)
    expect(hydrated.adjustments.maskExposure).toBe(.5)
    expect(toNativeSettings(hydrated.adjustments, [], 'sourceDefault', null, undefined, undefined,
      hydrated.layers, hydrated.mask!).layers).toEqual(changed.layers)
  })

  it('preserves modified legacy radial layers instead of dropping local controls during hydration', () => {
    const wire = toNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, [])
    wire.layers[0].adjustments.tone.contrast = .2
    wire.layers[0].opacity = .6
    wire.layers[0].name = 'My edited legacy mask'
    const original = JSON.stringify(wire.layers)
    const restored = fromNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, wire)
    expect(restored.adjustments.maskExposure).toBe(0)
    expect(restored.mask).toBeNull()
    expect(restored.layers).toHaveLength(1)
    expect(toNativeLayers(restored.layers)).toEqual(wire.layers)
    expect(toNativeSettings(restored.adjustments, [], 'sourceDefault', null, undefined, undefined, restored.layers).layers).toEqual(wire.layers)
    expect(JSON.stringify(wire.layers)).toBe(original)
  })

  it('keeps a reordered legacy radial layer in its original graph position', () => {
    const wire = toNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, [])
    wire.layers.push(structuredClone(localLayersFixture[1]) as NativeSerializedAdjustmentLayer)
    const restored = fromNativeSettings({ ...defaultAdjustments, maskExposure: 1 }, wire)
    expect(restored.adjustments.maskExposure).toBe(0)
    expect(restored.layers.map((layer) => layer.id)).toEqual(wire.layers.map((layer) => layer.id))
    expect(toNativeSettings(restored.adjustments, [], 'sourceDefault', null, undefined, undefined, restored.layers).layers).toEqual(wire.layers)
  })

  it('undoes and redoes rotation through native history rather than retaining current geometry', () => {
    const original = toNativeSettings(defaultAdjustments, [])
    const edited = toNativeSettings({ ...defaultAdjustments, rotation: 90, flipHorizontal: 1, mixerRedHue: 30 }, [])
    const undo = fromNativeSettings({ ...defaultAdjustments, rotation: 90, flipHorizontal: 1, mixerRedHue: 30 }, original)
    expect(undo.adjustments.rotation).toBe(0)
    expect(undo.adjustments.flipHorizontal).toBe(0)
    expect(undo.adjustments.mixerRedHue).toBe(0)
    expect(toNativeSettings(undo.adjustments, [])).toEqual(original)
    const redo = fromNativeSettings(undo.adjustments, edited)
    expect(redo.adjustments.rotation).toBe(90)
    expect(redo.adjustments.flipHorizontal).toBe(1)
    expect(toNativeSettings(redo.adjustments, [])).toEqual(edited)
  })

  it('hydrates every serialized adjustment family without leaving stale current-frame controls', () => {
    const original = toNativeSettings({ ...defaultAdjustments,
      exposure: .7, contrast: 12, highlights: -15, shadows: 23, whites: -4, blacks: -7,
      temperature: 21, tint: -8, vibrance: 15, saturation: -10,
      rotation: 90, flipHorizontal: 1, flipVertical: 1,
      geometryVertical: 13, geometryHorizontal: -17, geometryScale: 115, geometryOffsetX: 8, geometryOffsetY: -6,
      cropLeft: 10, cropTop: 20, cropRight: 90, cropBottom: 80, cropAspectWidth: 3, cropAspectHeight: 2,
      geometryUpright: 3, geometryFourPoint: 1, quadTopLeftX: 7, quadTopLeftY: 9,
      quadTopRightX: 92, quadTopRightY: 11, quadBottomRightX: 91, quadBottomRightY: 94,
      quadBottomLeftX: 5, quadBottomLeftY: 89,
      sharpness: 34, sharpenRadius: 1.8, sharpenDetail: 70, sharpenMasking: 42, sharpenHaloProtection: 88,
      noiseReduction: 10, denoiseLuminance: 29, denoiseChroma: 31, denoiseRadius: 1.7, denoiseDetailProtection: 61, denoiseHighIso: 25,
      texture: 14, clarity: -19, dehaze: 11,
      gradeGlobalHue: 31, gradeGlobalChroma: 12, gradeGlobalLightness: -3,
      gradeShadowsHue: -21, gradeShadowsChroma: 7, gradeShadowsLightness: 5,
      gradeMidtonesHue: 51, gradeMidtonesChroma: 8, gradeMidtonesLightness: -4,
      gradeHighlightsHue: 71, gradeHighlightsChroma: 9, gradeHighlightsLightness: 2,
      gradeBalance: 13, gradeBlending: 67, gradeAmount: 83,
      mixerRedHue: 19, mixerGreenChroma: -12, mixerBlueLightness: 14, mixerHueLock: 0,
      lensCorrection: 1, lensDistortion: 0, lensTca: 0, lensVignette: 0, lensAutoScale: 0,
      aiDenoiseEnabled: 1, aiDenoiseAmount: 41, aiDenoiseDetail: 62, aiDenoiseColorNoise: 73, aiDenoisePreserveSkin: 84,
      grainAmount: 21, grainSize: 31, grainRoughness: 61, grainColor: 41,
      vignette: -18, vignetteMidpoint: 42, vignetteRoundness: -16, vignetteFeather: 73, vignetteHighlightProtect: 28,
    }, [])
    const stale = Object.fromEntries(Object.keys(defaultAdjustments).map((key) => [key, 99])) as typeof defaultAdjustments
    const hydrated = fromNativeSettings(stale, JSON.parse(JSON.stringify(original)))
    expect(toNativeSettings(hydrated.adjustments, hydrated.curves.master, 'sourceDefault', null,
      hydrated.curves, undefined, hydrated.layers, hydrated.mask ?? defaultMask)).toEqual(original)
    const neutral = toNativeSettings(defaultAdjustments, [])
    const undo = fromNativeSettings(hydrated.adjustments, neutral)
    expect(toNativeSettings(undo.adjustments, undo.curves.master, 'sourceDefault', null,
      undo.curves, undefined, undo.layers)).toEqual(neutral)
    expect(undo.adjustments.geometryFourPoint).toBe(0)
    expect(undo.adjustments.quadTopLeftX).toBe(defaultAdjustments.quadTopLeftX)
  })

  it.each<NativeMaskTree>([
    { type: 'none' },
    { type: 'radial', x: .5, y: .4, width: .3, height: .6, rotation: 15, feather: .2, invert: false },
    { type: 'linear', startX: .1, startY: .2, endX: .8, endY: .7, feather: .15, invert: true },
    { type: 'brush', points: [{ x: .2, y: .3, pressure: .8 }], radius: .03, feather: .2, flow: .8, erase: false },
    { type: 'luminance', minimum: .1, maximum: .6, feather: .2, invert: false },
    { type: 'colorRange', reference: [.4, .2, .1], tolerance: .1, feather: .2, invert: false },
    { operation: 'invert', children: [{ type: 'none' }] },
  ])('serializes local tone for every manual/composite mask shape: %j', (mask) => {
    const layer: NativeAdjustmentLayer = { id: 'manual', name: 'Manual mask', enabled: true, opacity: .8,
      blendMode: 'normal', mask, adjustments: { tone: { exposureEv: .55, contrast: .1,
        highlights: -.2, shadows: .3, whites: -.1, blacks: -.05 } } }
    const request = toNativeSettings(defaultAdjustments, [], 'sourceDefault', null, undefined, undefined, [layer])
    expect(request.layers[0].adjustments.tone).toEqual({ exposure_ev: .55, contrast: .1,
      highlights: -.2, shadows: .3, whites: -.1, blacks: -.05 })
    expect(request.layers[0].mask).toEqual(toNativeMask(mask))
    expect(fromNativeLayers(request.layers)).toEqual([layer])
  })

  it('rejects malformed or conflicting local tone rather than silently substituting exposure', () => {
    const layer = structuredClone(localLayersFixture[0]) as NativeSerializedAdjustmentLayer
    for (const value of [NaN, Infinity, -Infinity]) {
      expect(() => fromNativeLayers([{ ...layer, adjustments: { tone: { ...layer.adjustments.tone, exposure_ev: value } } }]))
        .toThrow('NativeLayerContractInvalid')
    }
    const missing = structuredClone(layer) as unknown as NativeAdjustmentLayer
    Reflect.deleteProperty(missing.adjustments.tone, 'exposure_ev')
    expect(() => toNativeLayers([missing])).toThrow('NativeLayerContractInvalid')
    const conflicting = { ...layer, adjustments: { tone: { ...layer.adjustments.tone, exposureEv: .9 } } }
    expect(() => fromNativeLayers([conflicting])).toThrow('NativeLayerContractInvalid')
    const missingTone = { ...layer, adjustments: { tone: null } } as unknown as NativeSerializedAdjustmentLayer
    expect(() => fromNativeLayers([missingTone])).toThrow('NativeLayerContractInvalid')
  })
})
