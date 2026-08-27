export type Theme = 'dark' | 'gray' | 'light'
export type Tool = 'light' | 'color' | 'curve' | 'detail' | 'looks' | 'masks' | 'heal' | 'optics' | 'geometry'

export type AdjustmentKey =
  | 'exposure'
  | 'contrast'
  | 'highlights'
  | 'shadows'
  | 'whites'
  | 'blacks'
  | 'temperature'
  | 'tint'
  | 'vibrance'
  | 'saturation'
  | 'sharpness'
  | 'clarity'
  | 'noiseReduction'
  | 'maskExposure'
  | 'maskFeather'
  | 'vignette'
  | 'lensBrightness'
  | 'rotation'
  | 'flipHorizontal'
  | 'flipVertical'
  | 'mixerRedHue' | 'mixerRedChroma' | 'mixerRedLightness'
  | 'mixerOrangeHue' | 'mixerOrangeChroma' | 'mixerOrangeLightness'
  | 'mixerYellowHue' | 'mixerYellowChroma' | 'mixerYellowLightness'
  | 'mixerGreenHue' | 'mixerGreenChroma' | 'mixerGreenLightness'
  | 'mixerCyanHue' | 'mixerCyanChroma' | 'mixerCyanLightness'
  | 'mixerBlueHue' | 'mixerBlueChroma' | 'mixerBlueLightness'
  | 'mixerPurpleHue' | 'mixerPurpleChroma' | 'mixerPurpleLightness'
  | 'mixerMagentaHue' | 'mixerMagentaChroma' | 'mixerMagentaLightness'
  | 'mixerHueLock'
  | 'gradeGlobalHue' | 'gradeGlobalChroma' | 'gradeGlobalLightness'
  | 'gradeShadowsHue' | 'gradeShadowsChroma' | 'gradeShadowsLightness'
  | 'gradeMidtonesHue' | 'gradeMidtonesChroma' | 'gradeMidtonesLightness'
  | 'gradeHighlightsHue' | 'gradeHighlightsChroma' | 'gradeHighlightsLightness'
  | 'gradeBalance' | 'gradeBlending' | 'gradeAmount'
  | 'sharpenRadius' | 'sharpenDetail' | 'sharpenMasking' | 'sharpenHaloProtection'
  | 'denoiseLuminance' | 'denoiseChroma' | 'denoiseRadius' | 'denoiseDetailProtection' | 'denoiseHighIso'
  | 'aiDenoiseEnabled' | 'aiDenoiseAmount' | 'aiDenoiseDetail' | 'aiDenoiseColorNoise' | 'aiDenoisePreserveSkin'
  | 'grainAmount' | 'grainSize' | 'grainRoughness' | 'grainColor'
  | 'vignetteMidpoint' | 'vignetteRoundness' | 'vignetteFeather' | 'vignetteHighlightProtect'
  | 'texture' | 'dehaze'
  | 'lensCorrection' | 'lensDistortion' | 'lensTca' | 'lensVignette' | 'lensAutoScale'
  | 'geometryScale' | 'geometryOffsetX' | 'geometryOffsetY' | 'geometryVertical' | 'geometryHorizontal'
  | 'cropLeft' | 'cropTop' | 'cropRight' | 'cropBottom' | 'cropAspectWidth' | 'cropAspectHeight'
  | 'geometryFourPoint' | 'geometryUpright'
  | 'quadTopLeftX' | 'quadTopLeftY' | 'quadTopRightX' | 'quadTopRightY'
  | 'quadBottomRightX' | 'quadBottomRightY' | 'quadBottomLeftX' | 'quadBottomLeftY'

export type Adjustments = Record<AdjustmentKey, number>

export const defaultAdjustments: Adjustments = {
  exposure: 0,
  contrast: 0,
  highlights: 0,
  shadows: 0,
  whites: 0,
  blacks: 0,
  // Encoded JPEG/PNG/TIFF files do not expose a physical RAW Kelvin state here.
  // Temperature/Tint are therefore relative creative corrections in the -100..100 UI domain.
  temperature: 0,
  tint: 0,
  vibrance: 0,
  saturation: 0,
  sharpness: 0,
  clarity: 0,
  noiseReduction: 0,
  maskExposure: 0,
  maskFeather: 50,
  vignette: 0,
  lensBrightness: 0,
  rotation: 0,
  flipHorizontal: 0,
  flipVertical: 0,
  mixerRedHue: 0, mixerRedChroma: 0, mixerRedLightness: 0,
  mixerOrangeHue: 0, mixerOrangeChroma: 0, mixerOrangeLightness: 0,
  mixerYellowHue: 0, mixerYellowChroma: 0, mixerYellowLightness: 0,
  mixerGreenHue: 0, mixerGreenChroma: 0, mixerGreenLightness: 0,
  mixerCyanHue: 0, mixerCyanChroma: 0, mixerCyanLightness: 0,
  mixerBlueHue: 0, mixerBlueChroma: 0, mixerBlueLightness: 0,
  mixerPurpleHue: 0, mixerPurpleChroma: 0, mixerPurpleLightness: 0,
  mixerMagentaHue: 0, mixerMagentaChroma: 0, mixerMagentaLightness: 0,
  mixerHueLock: 1,
  gradeGlobalHue: 0, gradeGlobalChroma: 0, gradeGlobalLightness: 0,
  gradeShadowsHue: 0, gradeShadowsChroma: 0, gradeShadowsLightness: 0,
  gradeMidtonesHue: 0, gradeMidtonesChroma: 0, gradeMidtonesLightness: 0,
  gradeHighlightsHue: 0, gradeHighlightsChroma: 0, gradeHighlightsLightness: 0,
  gradeBalance: 0, gradeBlending: 50, gradeAmount: 100,
  sharpenRadius: 1, sharpenDetail: 50, sharpenMasking: 0, sharpenHaloProtection: 75,
  denoiseLuminance: 0, denoiseChroma: 0, denoiseRadius: 1.25, denoiseDetailProtection: 50, denoiseHighIso: 0,
  aiDenoiseEnabled: 0, aiDenoiseAmount: 50, aiDenoiseDetail: 50, aiDenoiseColorNoise: 50, aiDenoisePreserveSkin: 50,
  grainAmount: 0, grainSize: 50, grainRoughness: 50, grainColor: 0,
  vignetteMidpoint: 50, vignetteRoundness: 0, vignetteFeather: 50, vignetteHighlightProtect: 0,
  texture: 0, dehaze: 0,
  lensCorrection: 0, lensDistortion: 1, lensTca: 1, lensVignette: 1, lensAutoScale: 1,
  geometryScale: 100, geometryOffsetX: 0, geometryOffsetY: 0, geometryVertical: 0, geometryHorizontal: 0,
  cropLeft: 0, cropTop: 0, cropRight: 100, cropBottom: 100, cropAspectWidth: 0, cropAspectHeight: 0,
  geometryFourPoint: 0, geometryUpright: 0,
  quadTopLeftX: 0, quadTopLeftY: 0, quadTopRightX: 100, quadTopRightY: 0,
  quadBottomRightX: 100, quadBottomRightY: 100, quadBottomLeftX: 0, quadBottomLeftY: 100,
}

export interface EditorSnapshot {
  adjustments: Adjustments
}

export interface EditorState extends EditorSnapshot {
  history: EditorSnapshot[]
  future: EditorSnapshot[]
}

export const initialEditorState: EditorState = {
  adjustments: defaultAdjustments,
  history: [],
  future: [],
}

export type EditorAction =
  | { type: 'adjust'; key: AdjustmentKey; value: number; commit?: boolean }
  | { type: 'reset'; key: AdjustmentKey }
  | { type: 'undo' }
  | { type: 'redo' }

export function editorReducer(state: EditorState, action: EditorAction): EditorState {
  if (action.type === 'undo') {
    const previous = state.history.at(-1)
    if (!previous) return state
    return {
      adjustments: previous.adjustments,
      history: state.history.slice(0, -1),
      future: [{ adjustments: state.adjustments }, ...state.future],
    }
  }

  if (action.type === 'redo') {
    const next = state.future[0]
    if (!next) return state
    return {
      adjustments: next.adjustments,
      history: [...state.history, { adjustments: state.adjustments }],
      future: state.future.slice(1),
    }
  }

  const key = action.key
  const value = action.type === 'reset' ? defaultAdjustments[key] : action.value
  if (state.adjustments[key] === value) return state

  return {
    adjustments: { ...state.adjustments, [key]: value },
    history: action.type === 'reset' || action.commit
      ? [...state.history, { adjustments: state.adjustments }].slice(-100)
      : state.history,
    future: [],
  }
}
