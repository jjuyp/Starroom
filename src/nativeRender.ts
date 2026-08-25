import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import type { Adjustments } from './editorState'
import type { RadialMask, ToneCurvePoint } from './imagePipeline'

export type RenderBackend = 'native' | 'browserFallback'
export type NativeAssetFlag = 'unflagged' | 'pick' | 'reject'
export type NativeColorLabel = 'none' | 'red' | 'yellow' | 'green' | 'blue' | 'purple'
export type NativeSmartPredicate = { rating: { minimum: number } } | { flag: { value: NativeAssetFlag } } | { colorLabel: { value: NativeColorLabel } } | { camera: { value: string } } | { lens: { value: string } } | { fileType: { value: string } } | { keyword: { value: string } }
export interface NativeLibraryCollection { id: number; name: string; kind: 'normal' | 'smart'; rule: { all: NativeSmartPredicate[] } | null }
export interface NativeLibraryAsset {
  id: number; sourcePath: string; sourceIdentity: string; contentFingerprint: string
  fileSize: number; modifiedTime: number; rating: number; flag: NativeAssetFlag
  colorLabel: NativeColorLabel; missing: boolean; projectReference: string | null
  thumbnailCacheKey: string | null; keywords: string[]
  metadata: { fileType: string; width: number | null; height: number | null; orientation: number | null
    captureTime: number | null; cameraMake: string | null; cameraModel: string | null
    lensMake: string | null; lensModel: string | null; focalLength: number | null
    aperture: number | null; shutterSpeed: number | null; iso: number | null }
}
export interface NativeLibraryQuery {
  text?: string | null; filename?: string | null; camera?: string | null; lens?: string | null
  keyword?: string | null; minimumRating?: number | null; flag?: NativeAssetFlag | null
  colorLabel?: NativeColorLabel | null; fileTypes?: string[]; minimumIso?: number | null
  maximumIso?: number | null; captureFrom?: number | null; captureTo?: number | null
  missing?: boolean | null; sort?: 'captureTime' | 'importTime' | 'filename' | 'rating'
  direction?: 'ascending' | 'descending'; limit?: number; offset?: number
}
export interface NativeHistoryEntry { sequence: number; timestamp: number; description: string; affectedStage: string; version: string }
export interface NativeNamedSnapshot { id: string; name: string; createdAt: number; stateVersion: string; state: NativeEditSettings }
export interface NativeHistoryResult { state: NativeEditSettings; canUndo: boolean; canRedo: boolean; entries: NativeHistoryEntry[]; snapshots: NativeNamedSnapshot[]; stateVersion: string }
export interface NativeProfessionalExportSettings {
  format: 'jpeg' | 'png' | 'tiff'; bitDepth: 8 | 16; quality: number
  colorSpace: 'srgb' | 'displayP3' | 'adobeRgb' | 'rec2020'; embedProfile: boolean
  resize: { mode: 'original' } | { mode: 'width' | 'height' | 'longEdge' | 'shortEdge'; pixels: number }
    | { mode: 'percentage'; percent: number } | { mode: 'fitWithin'; width: number; height: number }
  outputSharpen: 'off' | 'screen' | 'print'; sharpenAmount: 'low' | 'standard' | 'high'
  metadata: 'allMetadata' | 'copyrightOnly' | 'cameraMetadata' | 'none'; includeLocation: boolean
  copyright: string | null; filenameTemplate: string; collision: 'fail' | 'autoRename' | 'overwrite'
}
export interface NativeProfessionalExportItem {
  assetId: number; sourcePath: string; originalName: string; captureDate: string | null; rating: number
  keywords: string[]
  camera: string | null; look: string | null; sequence: number; sourceFingerprint: string
  editStateIdentity: string; editSettings: NativeEditSettings
}
export interface NativeBatchExportResult { completed: unknown[]; failed: unknown[]; cancelled: unknown[]; skipped: unknown[] }
export interface NativeGpuStatus { backend: 'dx12' | 'other' | 'cpuFallback'; adapterName: string | null; reason: string | null }
export type NativeWhiteBalanceMode = 'sourceDefault' | 'asShot' | 'camera' | 'auto' | 'neutralPicker' | 'relative'
export interface NativeWhiteBalanceSample { x: number; y: number; width: number; height: number }
export interface NativeToneCurves { master: ToneCurvePoint[]; red: ToneCurvePoint[]; green: ToneCurvePoint[]; blue: ToneCurvePoint[] }
export type NativeColorBand = 'red' | 'orange' | 'yellow' | 'green' | 'cyan' | 'blue' | 'purple' | 'magenta'
export interface NativeBandAdjustment { hueDegrees: number; chroma: number; lightness: number }
export interface NativeColorMixer { bands: NativeBandAdjustment[]; hueLock: boolean; bandWidthDegrees: number }
export interface NativeColorWheel { hueDegrees: number; chroma: number; lightness: number }
export interface NativeGrading { shadows: NativeColorWheel; midtones: NativeColorWheel; highlights: NativeColorWheel; global: NativeColorWheel; balance: number; blending: number; amount: number }
export interface NativeLensIdentity { cameraMake: string; cameraModel: string; lensMake: string; lensModel: string; focalLengthMm: number; aperture: number; focusDistanceM: number | null }
export type NativeLensMatchMode = 'auto' | 'manual'
export interface NativeOpticsState { matchMode: NativeLensMatchMode; manualIdentity: NativeLensIdentity | null }
export interface NativeLensProfileResolution { status: 'autoMatched' | 'manualMatched' | 'missingMetadata' | 'unknownCamera' | 'unknownLens' | 'mountMismatch' | 'ambiguous'; profileId: string | null; databaseVersion: string; cameraMount: string | null; correction: unknown | null }
export const defaultNativeOpticsState: NativeOpticsState = { matchMode: 'auto', manualIdentity: null }
export type NativePortraitRegion = 'face' | 'skin' | 'eyes' | 'leftEye' | 'rightEye' | 'brows' | 'leftBrow' | 'rightBrow' | 'lips' | 'mouth' | 'hair'
export interface NativePortraitLandmark { x: number; y: number; z: number }
export interface NativePortraitFace { id: string; confidence: number; bounds: { left: number; top: number; right: number; bottom: number }; landmarks: NativePortraitLandmark[]; crop: { centerX: number; centerY: number; side: number; rotationDegrees: number } }
export interface NativePortraitFailure { code: string; message: string }
export interface NativePortraitDetection { status: 'ready' | 'noFace' | 'unavailable' | 'failed'; faces: Array<{ face: NativePortraitFace; cacheKey: string }>; detectorModelId: string; detectorModelVersion: string; detectorModelHash: string; parserModelId: string; parserModelVersion: string; parserModelHash: string; executionProvider: 'cpu' | 'directMl'; error: NativePortraitFailure | null }
export type NativeAiMaskSemantic = 'subject' | 'background' | 'person' | 'sky' | 'skin' | 'hair'
export interface NativeAiMaskResult { status: 'ready' | 'cached'; providerId: string; modelId: string; modelVersion: string; modelHash: string; semanticClass: NativeAiMaskSemantic; cacheIdentity: string; executionProvider: 'cpu' | 'directMl' }
export type NativeMaskDefinition =
  | { type: 'none' }
  | { type: 'radial'; x: number; y: number; width: number; height: number; rotation: number; feather: number; invert: boolean }
  | { type: 'linear'; startX: number; startY: number; endX: number; endY: number; feather: number; invert: boolean }
  | { type: 'brush'; points: Array<{ x: number; y: number; pressure: number }>; radius: number; feather: number; flow: number; erase: boolean }
  | { type: 'luminance'; minimum: number; maximum: number; feather: number; invert: boolean }
  | { type: 'colorRange'; reference: [number, number, number]; tolerance: number; feather: number; invert: boolean }
  | { type: 'portraitSemantic'; faceId: string; region: NativePortraitRegion; threshold: number; feather: number; modelId: string; modelVersion: string; modelHash: string; cacheKey: string }
  | { type: 'generated'; providerId: string; modelId: string; modelVersion: string; modelHash: string; semanticClass: NativeAiMaskSemantic; threshold: number; feather: number; invert: boolean; cacheIdentity: string; metadata: Record<string, string> }
export type NativeMaskTree = NativeMaskDefinition | { operation: 'add' | 'subtract' | 'intersect' | 'invert'; children: NativeMaskTree[] }
export interface NativeLayerAdjustments { tone: { exposureEv: number; contrast: number; highlights: number; shadows: number; whites: number; blacks: number } }
export interface NativeAdjustmentLayer { id: string; name: string; enabled: boolean; opacity: number; blendMode: 'normal'; mask: NativeMaskTree; adjustments: NativeLayerAdjustments }
export interface NativeSkinRetouchParameters { smooth: number; texture: number; toneEvenness: number; hueDegrees: number; chroma: number; exposureEv: number }
export interface NativeSkinRetouchFace { faceId: string; cacheKey: string }
export interface NativeSkinRetouchSettings { parameters: NativeSkinRetouchParameters; faces: NativeSkinRetouchFace[] }
export const defaultNativeSkinRetouch = (): NativeSkinRetouchSettings => ({ parameters: { smooth: 0, texture: .7, toneEvenness: 0, hueDegrees: 0, chroma: 0, exposureEv: 0 }, faces: [] })
export type NativeHealingMode = 'clone' | 'heal' | 'aiInpaint'
export type NativeHealingSourceMode = 'auto' | 'manual'
export interface NativeHealingOperation { id: string; enabled: boolean; mode: NativeHealingMode; target: { x: number; y: number }; source: { x: number; y: number } | null; radius: number; feather: number; opacity: number; rotationDegrees: number; scale: number; toneAdaptation: boolean; textureAdaptation: boolean; sourceMode: NativeHealingSourceMode; metadata: Record<string, string> }

export interface NativeEditSettings {
  exposure: number
  contrast: number
  highlights: number
  shadows: number
  whites: number
  blacks: number
  temperature: number
  tint: number
  vibrance: number
  saturation: number
  sharpness: number
  noiseReduction: number
  whiteBalanceMode: NativeWhiteBalanceMode
  whiteBalanceSample: NativeWhiteBalanceSample | null
  curve: Array<{ x: number; y: number }>
  curves: { master: Array<{ x: number; y: number }>; red: Array<{ x: number; y: number }>; green: Array<{ x: number; y: number }>; blue: Array<{ x: number; y: number }> }
  colorMixer: NativeColorMixer
  grading: NativeGrading
  sharpenSettings: { amount: number; radius: number; detail: number; masking: number; haloProtection: number; threshold: number }
  denoiseSettings: { luminance: number; chroma: number; radius: number; detailProtection: number; highIso: number }
  aiDenoise: { enabled: boolean; amount: number; detail: number; colorNoise: number; preserveSkin: number }
  aiDenoiseProvider: 'directMl' | 'cpu'
  localDetail: { texture: number; clarity: number; dehaze: number }
  optics: { parameters: { enabled: boolean; distortion: boolean; tca: boolean; vignette: boolean; autoScale: boolean }; matchMode: NativeLensMatchMode; manualIdentity: NativeLensIdentity | null }
  geometry: { rotationDegrees: number; verticalKeystone: number; horizontalKeystone: number; scale: number; offsetX: number; offsetY: number;
    flipHorizontal: boolean; flipVertical: boolean; crop: { left: number; top: number; right: number; bottom: number };
    cropAspectWidth: number; cropAspectHeight: number; fourPoint: null | { topLeft: { x: number; y: number }; topRight: { x: number; y: number }; bottomRight: { x: number; y: number }; bottomLeft: { x: number; y: number } };
    uprightMode: 'off' | 'auto' | 'level' | 'vertical' | 'full' }
  layers: NativeAdjustmentLayer[]
  skinRetouch: NativeSkinRetouchSettings
  healingOperations: NativeHealingOperation[]
  grain: { amount: number; size: number; roughness: number; color: number; seed: number }
  vignette: { amount: number; midpoint: number; roundness: number; feather: number; highlightProtect: number }
}

export interface NativePreviewResult {
  width: number
  height: number
  /** M12 is explicit: native is the shared graph, and this reports whether its Exposure node
   * executed on wgpu or on the CPU reference fallback. */
  acceleration: 'gpu' | 'cpuFallback'
  inputProfile: 'embedded ICC' | 'assumed sRGB' | 'resolved RAW camera profile' | 'Generic RAW Profile'
  cameraProfileId: string | null
  jpeg: Uint8Array
}
export type NativeAdvisorCategory = 'light' | 'whiteBalance' | 'color' | 'detail' | 'portrait'
export type NativeAdvisorConfidence = 'low' | 'medium' | 'high'
export interface NativeAdvisorSuggestion { id: string; category: NativeAdvisorCategory; control: string; amount: number; what: string; why: string; confidence: NativeAdvisorConfidence }
export interface NativeAdvisorResult { analysis: { luminanceMean: number; luminanceMedian: number; p01: number; p05: number; p25: number; p50: number; p75: number; p95: number; p99: number; blackClipFraction: number; whiteClipFraction: number; globalContrast: number; meanChroma: number; highChromaFraction: number; warmthBias: number; greenMagentaBias: number; portraitLuminanceMean: number; portraitChromaMean: number; portraitSampleFraction: number }; suggestions: NativeAdvisorSuggestion[] }

export interface NativeExportResult {
  outputPath: string
  width: number
  height: number
  inputProfile: string
  workingSpace: string
  cameraProfileHash: string | null
}
export interface NativeReferenceMatchResponse { settings: NativeEditSettings; recipe: { confidence: number; protectSkin: number }; sourceAnalysis: { fingerprint: string }; referenceAnalysis: { fingerprint: string } }

const HEADER_BYTES = 24

export const nativeRuntimeAvailable = () => isTauri()

/** Explicit M12 acceleration/fallback status for UI badges and diagnostics. */
export const getNativeGpuStatus = (preferGpu = true) =>
  invoke<NativeGpuStatus>('gpu_preview_status', { preferGpu })

export function assertNativeSupported(adjustments: Adjustments, mask: RadialMask) {
  // Kept in the boundary contract: M15 uses this same interaction geometry to construct the
  // native radial layer below, so the browser never composites it itself.
  void mask
  const unsupported: string[] = []
  if (adjustments.lensBrightness !== 0) unsupported.push('Optics')
  if (unsupported.length) {
    throw new Error(`Native M1C does not support ${unsupported.join(', ')} yet; Browser fallback was not used.`)
  }
}

export function toNativeSettings(adjustments: Adjustments, curve: ToneCurvePoint[],
  whiteBalanceMode: NativeWhiteBalanceMode = 'sourceDefault', whiteBalanceSample: NativeWhiteBalanceSample | null = null,
  toneCurves: NativeToneCurves = { master: curve, red: [], green: [], blue: [] },
  opticsState: NativeOpticsState = defaultNativeOpticsState,
  layers: NativeAdjustmentLayer[] = [],
  mask: RadialMask = { x: .5, y: .5, width: .42, height: .42, rotation: 0 },
  skinRetouch: NativeSkinRetouchSettings = defaultNativeSkinRetouch(),
  healingOperations: NativeHealingOperation[] = []): NativeEditSettings {
  const bands: NativeColorBand[] = ['red', 'orange', 'yellow', 'green', 'cyan', 'blue', 'purple', 'magenta']
  const title = (band: string) => `${band[0].toUpperCase()}${band.slice(1)}`
  const wheel = (zone: 'Global' | 'Shadows' | 'Midtones' | 'Highlights'): NativeColorWheel => ({
    hueDegrees: adjustments[`grade${zone}Hue`],
    chroma: adjustments[`grade${zone}Chroma`] / 100,
    lightness: adjustments[`grade${zone}Lightness`] / 100,
  })
  return {
    exposure: adjustments.exposure,
    contrast: adjustments.contrast,
    highlights: adjustments.highlights,
    shadows: adjustments.shadows,
    whites: adjustments.whites,
    blacks: adjustments.blacks,
    temperature: adjustments.temperature,
    tint: adjustments.tint,
    vibrance: adjustments.vibrance,
    saturation: adjustments.saturation,
    sharpness: adjustments.sharpness,
    noiseReduction: adjustments.noiseReduction,
    whiteBalanceMode,
    whiteBalanceSample,
    curve: [...curve].sort((left, right) => left.x - right.x).map(({ x, y }) => ({ x, y })),
    curves: Object.fromEntries(Object.entries(toneCurves).map(([channel, points]) => [channel, [...points].sort((left, right) => left.x - right.x).map(({ x, y }) => ({ x, y }))])) as NativeEditSettings['curves'],
    colorMixer: {
      bands: bands.map((band) => ({
        hueDegrees: adjustments[`mixer${title(band)}Hue` as keyof Adjustments],
        chroma: adjustments[`mixer${title(band)}Chroma` as keyof Adjustments] / 100,
        lightness: adjustments[`mixer${title(band)}Lightness` as keyof Adjustments] / 100,
      })),
      hueLock: adjustments.mixerHueLock !== 0,
      bandWidthDegrees: 52,
    },
    grading: {
      global: wheel('Global'), shadows: wheel('Shadows'), midtones: wheel('Midtones'), highlights: wheel('Highlights'),
      balance: adjustments.gradeBalance / 100, blending: adjustments.gradeBlending / 100, amount: adjustments.gradeAmount / 100,
    },
    sharpenSettings: {
      amount: Math.max(0, adjustments.sharpness / 50), radius: adjustments.sharpenRadius,
      detail: adjustments.sharpenDetail / 100, masking: adjustments.sharpenMasking / 100,
      haloProtection: adjustments.sharpenHaloProtection / 100, threshold: .002,
    },
    denoiseSettings: {
      luminance: Math.max(adjustments.noiseReduction, adjustments.denoiseLuminance) / 100,
      chroma: Math.max(adjustments.noiseReduction, adjustments.denoiseChroma) / 100,
      radius: adjustments.denoiseRadius, detailProtection: adjustments.denoiseDetailProtection / 100,
      highIso: adjustments.denoiseHighIso / 100,
    },
    aiDenoise: {
      enabled: adjustments.aiDenoiseEnabled !== 0,
      amount: adjustments.aiDenoiseAmount / 100,
      detail: adjustments.aiDenoiseDetail / 100,
      colorNoise: adjustments.aiDenoiseColorNoise / 100,
      preserveSkin: adjustments.aiDenoisePreserveSkin / 100,
    },
    aiDenoiseProvider: 'directMl',
    localDetail: { texture: adjustments.texture / 100, clarity: adjustments.clarity / 100, dehaze: adjustments.dehaze / 100 },
    optics: { parameters: { enabled: adjustments.lensCorrection !== 0, distortion: adjustments.lensDistortion !== 0,
      tca: adjustments.lensTca !== 0, vignette: adjustments.lensVignette !== 0, autoScale: adjustments.lensAutoScale !== 0 },
      matchMode: opticsState.matchMode, manualIdentity: opticsState.manualIdentity },
    geometry: {
      rotationDegrees: adjustments.rotation, verticalKeystone: adjustments.geometryVertical / 100,
      horizontalKeystone: adjustments.geometryHorizontal / 100, scale: adjustments.geometryScale / 100,
      offsetX: adjustments.geometryOffsetX / 100, offsetY: adjustments.geometryOffsetY / 100,
      flipHorizontal: adjustments.flipHorizontal !== 0, flipVertical: adjustments.flipVertical !== 0,
      crop: { left: adjustments.cropLeft / 100, top: adjustments.cropTop / 100,
        right: adjustments.cropRight / 100, bottom: adjustments.cropBottom / 100 },
      cropAspectWidth: adjustments.cropAspectWidth, cropAspectHeight: adjustments.cropAspectHeight,
      fourPoint: adjustments.geometryFourPoint === 0 ? null : {
        topLeft: { x: adjustments.quadTopLeftX / 100, y: adjustments.quadTopLeftY / 100 },
        topRight: { x: adjustments.quadTopRightX / 100, y: adjustments.quadTopRightY / 100 },
        bottomRight: { x: adjustments.quadBottomRightX / 100, y: adjustments.quadBottomRightY / 100 },
        bottomLeft: { x: adjustments.quadBottomLeftX / 100, y: adjustments.quadBottomLeftY / 100 },
      },
      uprightMode: (['off', 'auto', 'level', 'vertical', 'full'] as const)[Math.round(adjustments.geometryUpright)] ?? 'off',
    },
    layers: [
      ...layers.map((layer) => ({ ...layer, mask: structuredClone(layer.mask), adjustments: { tone: { ...layer.adjustments.tone } } })),
      ...(adjustments.maskExposure === 0 ? [] : [{
        id: '__m15-radial-mask__', name: 'Radial mask', enabled: true, opacity: 1, blendMode: 'normal' as const,
        mask: { type: 'radial' as const, ...mask, feather: Math.max(0, adjustments.maskFeather / 100), invert: false },
        adjustments: { tone: { exposureEv: adjustments.maskExposure, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } },
      }]),
    ],
    skinRetouch: { parameters: { ...skinRetouch.parameters }, faces: skinRetouch.faces.map((face) => ({ ...face })) },
    healingOperations: healingOperations.map((operation) => structuredClone(operation)),
    grain: { amount: adjustments.grainAmount / 100, size: adjustments.grainSize / 100,
      roughness: adjustments.grainRoughness / 100, color: adjustments.grainColor / 100, seed: 0 },
    vignette: { amount: adjustments.vignette / 100, midpoint: adjustments.vignetteMidpoint / 100,
      roundness: adjustments.vignetteRoundness / 100, feather: adjustments.vignetteFeather / 100,
      highlightProtect: adjustments.vignetteHighlightProtect / 100 },
  }
}

export function parseNativePreviewFrame(value: ArrayBuffer | Uint8Array): NativePreviewResult {
  const bytes = value instanceof Uint8Array ? value : new Uint8Array(value)
  if (bytes.byteLength < HEADER_BYTES || String.fromCharCode(...bytes.subarray(0, 4)) !== 'SRP2') {
    throw new Error('Native preview returned an invalid binary frame.')
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const version = view.getUint16(4, true)
  if (version !== 2) throw new Error(`Unsupported native preview contract version ${version}.`)
  const flags = view.getUint16(6, true)
  const width = view.getUint32(8, true)
  const height = view.getUint32(12, true)
  const profileLength = view.getUint16(16, true)
  const payloadLength = view.getUint32(20, true)
  if (!width || !height || HEADER_BYTES + profileLength + payloadLength !== bytes.byteLength) {
    throw new Error('Native preview returned inconsistent dimensions or payload length.')
  }
  const profileStart = HEADER_BYTES
  const payloadStart = profileStart + profileLength
  const cameraProfileId = profileLength
    ? new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(profileStart, payloadStart))
    : null
  return {
    width,
    height,
    acceleration: flags & 8 ? 'gpu' : 'cpuFallback',
    inputProfile: flags & 4 ? 'Generic RAW Profile'
      : flags & 2 ? 'resolved RAW camera profile'
        : flags & 1 ? 'embedded ICC' : 'assumed sRGB',
    cameraProfileId,
    jpeg: bytes.slice(payloadStart),
  }
}

export async function chooseNativePhotoPaths(): Promise<string[]> {
  const selected = await open({
    title: 'Add photos to Starroom',
    multiple: true,
    directory: false,
    filters: [{
      name: 'Photos and camera RAW',
      extensions: ['jpg', 'jpeg', 'png', 'tif', 'tiff', 'nef', 'arw', 'cr2', 'cr3', 'dng', 'raf'],
    }],
  })
  return selected ? (Array.isArray(selected) ? selected : [selected]) : []
}

export async function chooseNativeReferencePath(): Promise<string | null> {
  const selected = await open({ title: 'Choose reference photo', multiple: false, directory: false,
    filters: [{ name: 'Photo or camera RAW', extensions: ['jpg', 'jpeg', 'png', 'tif', 'tiff', 'nef', 'arw', 'cr2', 'cr3', 'dng', 'raf'] }] })
  return typeof selected === 'string' ? selected : null
}

export async function chooseNativeLookPath(mode: 'open' | 'save', suggestedName = 'Starroom Look.srlook'): Promise<string | null> {
  if (mode === 'open') {
    const selected = await open({ title: 'Open Starroom look', multiple: false, directory: false, filters: [{ name: 'Starroom Look', extensions: ['srlook'] }] })
    return typeof selected === 'string' ? selected : null
  }
  return save({ title: 'Save Starroom look', defaultPath: suggestedName, filters: [{ name: 'Starroom Look', extensions: ['srlook'] }] })
}

export interface NativeReferenceControls { amount: number; tone: number; color: number; grading: number; protectSkin: number }

export async function matchNativeReference(sourcePath: string, referencePath: string, settings: NativeEditSettings,
  controls: NativeReferenceControls = { amount: .7, tone: 1, color: 1, grading: 1, protectSkin: .8 }) {
  return invoke<NativeReferenceMatchResponse>('native_reference_match', {
    request: { sourcePath, referencePath, maxEdge: 1600, ...controls, settings },
  })
}

export async function saveNativeLook(path: string, name: string, settings: NativeEditSettings) {
  return invoke<string>('native_look_save', { request: { path, name, settings } })
}

export async function applyNativeLook(path: string, amount: number, settings: NativeEditSettings) {
  return invoke<NativeEditSettings>('native_look_apply', { request: { path, amount, settings } })
}

export async function mixNativeLooks(pathA: string, pathB: string, weightA: number, weightB: number,
  amount: number, settings: NativeEditSettings) {
  return invoke<NativeEditSettings>('native_look_mix', {
    request: { pathA, pathB, weightA, weightB, amount, settings },
  })
}

export function fromNativeSettings(base: Adjustments, settings: NativeEditSettings): { adjustments: Adjustments; curves: NativeToneCurves } {
  const adjustments: Adjustments = {
    ...base,
    exposure: settings.exposure, contrast: settings.contrast, highlights: settings.highlights, shadows: settings.shadows,
    whites: settings.whites, blacks: settings.blacks, temperature: settings.temperature, tint: settings.tint,
    vibrance: settings.vibrance, saturation: settings.saturation,
    aiDenoiseEnabled: settings.aiDenoise.enabled ? 1 : 0, aiDenoiseAmount: settings.aiDenoise.amount * 100,
    aiDenoiseDetail: settings.aiDenoise.detail * 100, aiDenoiseColorNoise: settings.aiDenoise.colorNoise * 100,
    aiDenoisePreserveSkin: settings.aiDenoise.preserveSkin * 100,
    grainAmount: settings.grain.amount * 100, grainSize: settings.grain.size * 100,
    grainRoughness: settings.grain.roughness * 100, grainColor: settings.grain.color * 100,
    vignette: settings.vignette.amount * 100, vignetteMidpoint: settings.vignette.midpoint * 100,
    vignetteRoundness: settings.vignette.roundness * 100, vignetteFeather: settings.vignette.feather * 100,
    vignetteHighlightProtect: settings.vignette.highlightProtect * 100,
  }
  const bands = ['Red', 'Orange', 'Yellow', 'Green', 'Cyan', 'Blue', 'Purple', 'Magenta'] as const
  bands.forEach((band, index) => {
    adjustments[`mixer${band}Hue` as keyof Adjustments] = settings.colorMixer.bands[index].hueDegrees
    adjustments[`mixer${band}Chroma` as keyof Adjustments] = settings.colorMixer.bands[index].chroma * 100
    adjustments[`mixer${band}Lightness` as keyof Adjustments] = settings.colorMixer.bands[index].lightness * 100
  })
  const withIds = (points: Array<{ x: number; y: number }>, channel: string) => points.map((point, index) => ({ ...point, id: `${channel}-${index}` }))
  return { adjustments, curves: { master: withIds(settings.curves.master, 'master'), red: withIds(settings.curves.red, 'red'), green: withIds(settings.curves.green, 'green'), blue: withIds(settings.curves.blue, 'blue') } }
}

/** M16: all inference is local Rust/ONNX Runtime. This returns compact geometry/cache metadata,
 * never parser pixels or image data through JSON. */
export async function detectNativePortrait(sourcePath: string, faceCropScale = 1.4): Promise<NativePortraitDetection> {
  return invoke<NativePortraitDetection>('portrait_detect', { request: { sourcePath, faceCropScale } })
}

export async function generateNativeAiMask(sourcePath: string, semantic: Extract<NativeAiMaskSemantic, 'subject' | 'background' | 'sky'>, requestId: string): Promise<NativeAiMaskResult> {
  return invoke<NativeAiMaskResult>('ai_mask_generate', { request: { sourcePath, semantic, requestId } })
}

export async function cancelNativeAiMask(requestId: string): Promise<boolean> {
  return invoke<boolean>('ai_mask_cancel', { requestId })
}

export async function cancelNativeAiDenoise(requestId: string): Promise<boolean> {
  return invoke<boolean>('ai_denoise_cancel', { requestId })
}

export const nativeThumbnailUrl = (path: string) => convertFileSrc(path)

let activeDenoisePreviewRequestId: string | null = null

export async function renderNativePreview(
  sourcePath: string,
  adjustments: Adjustments,
  curve: ToneCurvePoint[],
  mask: RadialMask,
  whiteBalanceMode: NativeWhiteBalanceMode = 'sourceDefault',
  whiteBalanceSample: NativeWhiteBalanceSample | null = null,
  toneCurves: NativeToneCurves = { master: curve, red: [], green: [], blue: [] },
  opticsState: NativeOpticsState = defaultNativeOpticsState,
  layers: NativeAdjustmentLayer[] = [],
  maxEdge = 1800,
  skinRetouch: NativeSkinRetouchSettings = defaultNativeSkinRetouch(),
  healingOperations: NativeHealingOperation[] = [],
) {
  assertNativeSupported(adjustments, mask)
  const requestId = crypto.randomUUID()
  const superseded = activeDenoisePreviewRequestId
  activeDenoisePreviewRequestId = requestId
  if (superseded) void cancelNativeAiDenoise(superseded)
  try {
    const frame = await invoke<ArrayBuffer | Uint8Array>('native_preview', {
      request: { requestId, sourcePath, maxEdge, settings: toNativeSettings(adjustments, curve, whiteBalanceMode, whiteBalanceSample, toneCurves, opticsState, layers, mask, skinRetouch, healingOperations) },
    })
    return parseNativePreviewFrame(frame)
  } finally {
    if (activeDenoisePreviewRequestId === requestId) activeDenoisePreviewRequestId = null
  }
}

export async function sampleNativeColor(sourcePath: string, x: number, y: number, adjustments: Adjustments,
  curve: ToneCurvePoint[], whiteBalanceMode: NativeWhiteBalanceMode, whiteBalanceSample: NativeWhiteBalanceSample | null,
  toneCurves: NativeToneCurves, opticsState: NativeOpticsState = defaultNativeOpticsState): Promise<NativeColorBand | null> {
  return invoke<NativeColorBand | null>('native_sample_color', {
    request: { sourcePath, x, y, settings: toNativeSettings(adjustments, curve, whiteBalanceMode, whiteBalanceSample, toneCurves, opticsState) },
  })
}

/** M19 local, explainable advisor. It invokes Rust's shared native graph and returns only
 * descriptive statistics plus bounded parameter suggestions—never image pixels or cloud data. */
export async function adviseNativeImage(sourcePath: string, adjustments: Adjustments, curve: ToneCurvePoint[],
  whiteBalanceMode: NativeWhiteBalanceMode, whiteBalanceSample: NativeWhiteBalanceSample | null,
  toneCurves: NativeToneCurves, opticsState: NativeOpticsState, layers: NativeAdjustmentLayer[] = [],
  skinRetouch: NativeSkinRetouchSettings = defaultNativeSkinRetouch(), healingOperations: NativeHealingOperation[] = []): Promise<NativeAdvisorResult> {
  return invoke<NativeAdvisorResult>('advise_native_image', { request: { sourcePath, maxEdge: 1024,
    settings: toNativeSettings(adjustments, curve, whiteBalanceMode, whiteBalanceSample, toneCurves, opticsState, layers, { x: .5, y: .5, width: .42, height: .42, rotation: 0 }, skinRetouch, healingOperations) } })
}

export async function chooseNativeExportPath(sourceName: string) {
  const base = sourceName.replace(/\.[^.]+$/, '')
  return save({
    title: 'Export Starroom JPEG',
    defaultPath: `${base}-starroom.jpg`,
    filters: [{ name: 'JPEG image', extensions: ['jpg', 'jpeg'] }],
  })
}

export async function openNativeLibrary() {
  return invoke<{ path: string; schemaVersion: number }>('library_open_default')
}

export async function chooseNativeLibraryFolder() {
  const value = await open({ title: 'Import folder into Starroom Library', directory: true, multiple: false })
  return typeof value === 'string' ? value : null
}

export async function importNativeLibraryFolder(root: string) {
  return invoke<{ imported: number[]; alreadyPresent: string[]; duplicates: string[]; relinkCandidates: Array<[number, string]>; unsupported: string[]; failed: Array<[string, string]>; cancelled: boolean }>('library_import_folder', { root })
}

export async function queryNativeLibrary(query: NativeLibraryQuery = {}) {
  return invoke<NativeLibraryAsset[]>('library_query', { query: {
    text: null, filename: null, camera: null, lens: null, keyword: null, minimumRating: null,
    flag: null, colorLabel: null, fileTypes: [], minimumIso: null, maximumIso: null,
    captureFrom: null, captureTo: null, missing: null, sort: 'importTime', direction: 'descending',
    limit: 200, offset: 0, ...query,
  } })
}

export async function updateNativeLibraryWorkflow(assetIds: number[], values: { rating?: number; flag?: NativeAssetFlag; colorLabel?: NativeColorLabel }) {
  return invoke<void>('library_set_workflow', { request: { assetIds, rating: values.rating ?? null, flag: values.flag ?? null, colorLabel: values.colorLabel ?? null } })
}

export async function addNativeLibraryKeywords(assetIds: number[], names: string[]) {
  return invoke<void>('library_add_keywords', { request: { assetIds, names } })
}
export async function removeNativeLibraryKeywords(assetIds: number[], names: string[]) { return invoke<void>('library_remove_keywords', { request: { assetIds, names } }) }
export async function nativeLibraryCollections() { return invoke<NativeLibraryCollection[]>('library_collections') }
export async function createNativeLibraryCollection(name: string, kind: 'normal' | 'smart', rule: { all: NativeSmartPredicate[] } | null = null) { return invoke<number>('library_collection_create', { request: { name, kind, rule } }) }
export async function addNativeLibraryCollectionAssets(collectionId: number, assetIds: number[]) { return invoke<void>('library_collection_add_assets', { collectionId, assetIds }) }
export async function nativeLibraryCollectionAssets(collectionId: number, limit = 200, offset = 0) { return invoke<NativeLibraryAsset[]>('library_collection_assets', { collectionId, limit, offset }) }

export async function nativeLibraryThumbnail(assetId: number, size: 'small256' | 'medium512' | 'large1024' = 'medium512') {
  const path = await invoke<string>('library_thumbnail', { assetId, size })
  return convertFileSrc(path)
}

export async function openNativeHistory(assetId: number, initialState: NativeEditSettings) {
  return invoke<NativeHistoryResult>('history_open', { assetId, initialState })
}

export async function commitNativeHistory(assetId: number, description: string, affectedStage: string, before: NativeEditSettings, after: NativeEditSettings) {
  return invoke<NativeHistoryResult>('history_commit', { request: { assetId, description, affectedStage, before, after } })
}

export async function undoNativeHistory(assetId: number) { return invoke<NativeHistoryResult>('history_undo', { assetId }) }
export async function redoNativeHistory(assetId: number) { return invoke<NativeHistoryResult>('history_redo', { assetId }) }
export async function createNativeSnapshot(assetId: number, name: string) { return invoke<NativeHistoryResult>('history_snapshot_create', { assetId, name }) }
export async function restoreNativeSnapshot(assetId: number, snapshotId: string) { return invoke<NativeHistoryResult>('history_snapshot_restore', { assetId, snapshotId }) }
export async function renameNativeSnapshot(assetId: number, snapshotId: string, name: string) { return invoke<NativeHistoryResult>('history_snapshot_rename', { assetId, snapshotId, name }) }
export async function deleteNativeSnapshot(assetId: number, snapshotId: string) { return invoke<NativeHistoryResult>('history_snapshot_delete', { assetId, snapshotId }) }

export async function chooseNativeExportDirectory() {
  const value = await open({ title: 'Choose Starroom export folder', directory: true, multiple: false })
  return typeof value === 'string' ? value : null
}

export async function exportNativeBatch(destinationDirectory: string, settings: NativeProfessionalExportSettings, items: NativeProfessionalExportItem[]) {
  return invoke<NativeBatchExportResult>('native_export_batch', { request: { destinationDirectory, settings, items } })
}

export async function cancelNativeExport() { return invoke<boolean>('native_export_cancel') }

export async function exportNativeJpeg(
  sourcePath: string,
  outputPath: string,
  adjustments: Adjustments,
  curve: ToneCurvePoint[],
  mask: RadialMask,
  whiteBalanceMode: NativeWhiteBalanceMode = 'sourceDefault',
  whiteBalanceSample: NativeWhiteBalanceSample | null = null,
  toneCurves: NativeToneCurves = { master: curve, red: [], green: [], blue: [] },
  opticsState: NativeOpticsState = defaultNativeOpticsState,
  layers: NativeAdjustmentLayer[] = [],
  skinRetouch: NativeSkinRetouchSettings = defaultNativeSkinRetouch(),
  healingOperations: NativeHealingOperation[] = [],
) {
  assertNativeSupported(adjustments, mask)
  return invoke<NativeExportResult>('native_export_jpeg', {
    request: { requestId: crypto.randomUUID(), sourcePath, outputPath, quality: 94, settings: toNativeSettings(adjustments, curve, whiteBalanceMode, whiteBalanceSample, toneCurves, opticsState, layers, mask, skinRetouch, healingOperations) },
  })
}

export async function resolveNativeOpticsStatus(sourcePath: string, adjustments: Adjustments, curve: ToneCurvePoint[],
  whiteBalanceMode: NativeWhiteBalanceMode, whiteBalanceSample: NativeWhiteBalanceSample | null,
  toneCurves: NativeToneCurves, opticsState: NativeOpticsState) {
  return invoke<NativeLensProfileResolution>('native_optics_status', {
    request: { sourcePath, settings: toNativeSettings(adjustments, curve, whiteBalanceMode, whiteBalanceSample, toneCurves, opticsState) },
  })
}
