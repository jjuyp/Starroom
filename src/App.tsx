import { useEffect, useMemo, useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import {
  Aperture, Blend, ChevronDown, Columns2, Contrast, Crop, Download, Folder,
  Grid2X2, ImagePlus, Library, PanelBottomClose, PanelBottomOpen, PanelLeftClose,
  PanelLeftOpen, Redo2, RotateCcw, RotateCw, ScanFace, ScanLine, Sparkles, Star,
  SunMedium, Trash2, Undo2, FlipHorizontal2, FlipVertical2, Move,
} from 'lucide-react'
import {
  type AdjustmentKey, type Adjustments, type Theme, type Tool,
  defaultAdjustments,
} from './editorState'
import {
  calculateHistogram, hasAdjustments, mapToneCurve, renderImageSource,
  type RadialMask, type ToneCurvePoint,
} from './imagePipeline'
import {
  adviseNativeImage, chooseNativeExportPath, chooseNativePhotoPaths, exportNativeJpeg, nativeRuntimeAvailable,
  nativeThumbnailUrl, renderNativePreview, sampleNativeColor, type NativeEditSettings, type NativeReferenceMatchResponse, type NativeToneCurves, type NativeWhiteBalanceMode, type NativeWhiteBalanceSample, type RenderBackend,
  defaultNativeOpticsState, resolveNativeOpticsStatus, type NativeLensIdentity, type NativeLensProfileResolution, type NativeOpticsState,
  cancelNativeAiMask, detectNativePortrait, generateNativeAiMask, defaultNativeSkinRetouch, type NativeAdjustmentLayer, type NativeAdvisorResult, type NativeAdvisorSuggestion, type NativeAiMaskResult, type NativeAiMaskSemantic, type NativeHealingOperation, type NativeMaskDefinition, type NativeMaskTree, type NativePortraitDetection, type NativePortraitRegion, type NativeSkinRetouchSettings,
  applyNativeLook, chooseNativeLookPath, chooseNativeReferencePath, fromNativeSettings, matchNativeReference, mixNativeLooks, saveNativeLook, toNativeSettings,
  addNativeLibraryKeywords, chooseNativeLibraryFolder, importNativeLibraryFolder, nativeLibraryThumbnail,
  openNativeLibrary, queryNativeLibrary, updateNativeLibraryWorkflow,
  type NativeLibraryAsset, type NativeAssetFlag, type NativeColorLabel,
  commitNativeHistory, createNativeSnapshot, openNativeHistory, redoNativeHistory, restoreNativeSnapshot, undoNativeHistory,
  type NativeHistoryResult,
} from './nativeRender'

type LibraryFilter = 'all' | 'recent' | 'five-star' | 'edited'
type WorkspaceView = 'library' | 'edit' | 'compare'

interface PhotoItem {
  id: string
  name: string
  src: string
  sourcePath?: string
  renderBackend: RenderBackend
  imported: boolean
  libraryAsset?: NativeLibraryAsset
  rating: number
  adjustments: Adjustments
  curvePoints: ToneCurvePoint[]
  curveChannels: NativeToneCurves
  whiteBalanceMode: NativeWhiteBalanceMode
  whiteBalanceSample: NativeWhiteBalanceSample | null
  opticsState: NativeOpticsState
  mask: RadialMask
  layers: NativeAdjustmentLayer[]
  skinRetouch: NativeSkinRetouchSettings
  healingOperations: NativeHealingOperation[]
  history: EditSnapshot[]
  future: EditSnapshot[]
}

interface EditSnapshot {
  adjustments: Adjustments
  curvePoints: ToneCurvePoint[]
  curveChannels: NativeToneCurves
  whiteBalanceMode: NativeWhiteBalanceMode
  whiteBalanceSample: NativeWhiteBalanceSample | null
  opticsState: NativeOpticsState
  mask: RadialMask
  layers: NativeAdjustmentLayer[]
  skinRetouch: NativeSkinRetouchSettings
  healingOperations: NativeHealingOperation[]
}

const defaultCurvePoints: ToneCurvePoint[] = [
  { id: 'black', x: 0, y: 0 },
  { id: 'shadow', x: .25, y: .25 },
  { id: 'midtone', x: .5, y: .5 },
  { id: 'highlight', x: .75, y: .75 },
  { id: 'white', x: 1, y: 1 },
]
const defaultMask: RadialMask = { x: .5, y: .5, width: .42, height: .42, rotation: 0 }

const copyCurve = (points: ToneCurvePoint[]) => points.map((point) => ({ ...point }))
const defaultCurveChannels = (): NativeToneCurves => ({ master: copyCurve(defaultCurvePoints), red: [], green: [], blue: [] })
const copyCurveChannels = (curves: NativeToneCurves): NativeToneCurves => ({ master: copyCurve(curves.master), red: copyCurve(curves.red), green: copyCurve(curves.green), blue: copyCurve(curves.blue) })
const defaultLayer = (): NativeAdjustmentLayer => ({ id: crypto.randomUUID(), name: 'Adjustment layer', enabled: true, opacity: 1, blendMode: 'normal', mask: { type: 'none' }, adjustments: { tone: { exposureEv: 0, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0 } } })
const copyLayers = (layers: NativeAdjustmentLayer[]) => layers.map((layer) => ({ ...layer, mask: structuredClone(layer.mask), adjustments: { tone: { ...layer.adjustments.tone } } }))
const copySkinRetouch = (value: NativeSkinRetouchSettings): NativeSkinRetouchSettings => ({ parameters: { ...value.parameters }, faces: value.faces.map((face) => ({ ...face })) })
const copyHealingOperations = (operations: NativeHealingOperation[]) => operations.map((operation) => structuredClone(operation))
const newMaskOfType = (type: 'none' | 'radial' | 'linear' | 'brush' | 'luminance' | 'colorRange'): NativeAdjustmentLayer['mask'] => {
  if (type === 'radial') return { type, x: .5, y: .5, width: .4, height: .4, rotation: 0, feather: .2, invert: false }
  if (type === 'linear') return { type, startX: .25, startY: .5, endX: .75, endY: .5, feather: .2, invert: false }
  if (type === 'brush') return { type, points: [{ x: .5, y: .5, pressure: 1 }], radius: .15, feather: .5, flow: 1, erase: false }
  if (type === 'luminance') return { type, minimum: .2, maximum: .8, feather: .05, invert: false }
  if (type === 'colorRange') return { type, reference: [.5, .5, .5], tolerance: .15, feather: .1, invert: false }
  return { type: 'none' }
}

function LibraryMetadataPanel({ asset, selectedCount, onWorkflow, onAddKeyword }: {
  asset: NativeLibraryAsset | null; selectedCount: number
  onWorkflow: (value: { rating?: number; flag?: NativeAssetFlag; colorLabel?: NativeColorLabel }) => void
  onAddKeyword: (keyword: string) => void
}) {
  const [keyword, setKeyword] = useState('')
  return <section className="library-metadata" aria-label="Library metadata">
    <div className="inspector-head"><div><span className="eyebrow">Library selection</span><h2>{selectedCount || 0} selected</h2></div></div>
    {!asset ? <div className="tool-note">Select a Library photo to inspect metadata and apply batch workflow fields.</div> : <>
      <dl><dt>File</dt><dd>{asset.sourcePath.split(/[\\/]/).pop()}</dd><dt>Type</dt><dd>{asset.metadata.fileType.toUpperCase()}</dd>
        <dt>Dimensions</dt><dd>{asset.metadata.width ?? '—'} × {asset.metadata.height ?? '—'}</dd>
        <dt>Camera</dt><dd>{[asset.metadata.cameraMake, asset.metadata.cameraModel].filter(Boolean).join(' ') || '—'}</dd>
        <dt>Lens</dt><dd>{[asset.metadata.lensMake, asset.metadata.lensModel].filter(Boolean).join(' ') || '—'}</dd>
        <dt>ISO</dt><dd>{asset.metadata.iso ?? '—'}</dd><dt>Status</dt><dd>{asset.missing ? 'Missing source' : 'Online'}</dd></dl>
      <label>Rating<select value={asset.rating} onChange={(event) => onWorkflow({ rating: Number(event.target.value) })}>{[0,1,2,3,4,5].map((value) => <option key={value} value={value}>{value ? `${value} star${value === 1 ? '' : 's'}` : 'Unrated'}</option>)}</select></label>
      <label>Flag<select value={asset.flag} onChange={(event) => onWorkflow({ flag: event.target.value as NativeAssetFlag })}><option value="unflagged">Unflagged</option><option value="pick">Pick</option><option value="reject">Reject</option></select></label>
      <label>Color label<select value={asset.colorLabel} onChange={(event) => onWorkflow({ colorLabel: event.target.value as NativeColorLabel })}>{['none','red','yellow','green','blue','purple'].map((value) => <option key={value} value={value}>{value}</option>)}</select></label>
      <div className="keyword-editor"><span>{asset.keywords.length ? asset.keywords.join(' · ') : 'No keywords'}</span><input value={keyword} placeholder="Add keyword" onChange={(event) => setKeyword(event.target.value)} /><button onClick={() => { if (keyword.trim()) { onAddKeyword(keyword); setKeyword('') } }}>Add</button></div>
    </>}
  </section>
}

/** UI edits only serializable native mask intent; all image math remains in Rust. */
function LayerMaskControls({ mask, onChange }: { mask: NativeMaskDefinition; onChange: (mask: NativeMaskDefinition) => void }) {
  const number = (label: string, value: number, update: (value: number) => NativeMaskDefinition, options: { min?: number; max?: number; step?: number } = {}) => (
    <label>{label}<input aria-label={`Mask ${label}`} type="number" value={value} min={options.min} max={options.max} step={options.step ?? .01}
      onChange={(event) => onChange(update(Number(event.target.value) || 0))} /></label>
  )
  const invert = (current: Exclude<NativeMaskDefinition, { type: 'none' }>, update: (invert: boolean) => NativeMaskDefinition) => (
    <label><input aria-label="Invert mask" type="checkbox" checked={'invert' in current ? current.invert : false}
      onChange={(event) => onChange(update(event.target.checked))} /> Invert</label>
  )
  if (mask.type === 'radial') return <div className="mask-controls">
    {number('Center X', mask.x, (x) => ({ ...mask, x }), { min: 0, max: 1 })}{number('Center Y', mask.y, (y) => ({ ...mask, y }), { min: 0, max: 1 })}
    {number('Width', mask.width, (width) => ({ ...mask, width }), { min: .001, max: 2 })}{number('Height', mask.height, (height) => ({ ...mask, height }), { min: .001, max: 2 })}
    {number('Angle', mask.rotation, (rotation) => ({ ...mask, rotation }), { min: -180, max: 180, step: 1 })}{number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 1 })}
    {invert(mask, (invert) => ({ ...mask, invert }))}
  </div>
  if (mask.type === 'linear') return <div className="mask-controls">
    {number('Start X', mask.startX, (startX) => ({ ...mask, startX }), { min: 0, max: 1 })}{number('Start Y', mask.startY, (startY) => ({ ...mask, startY }), { min: 0, max: 1 })}
    {number('End X', mask.endX, (endX) => ({ ...mask, endX }), { min: 0, max: 1 })}{number('End Y', mask.endY, (endY) => ({ ...mask, endY }), { min: 0, max: 1 })}
    {number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 1 })}{invert(mask, (invert) => ({ ...mask, invert }))}
  </div>
  if (mask.type === 'brush') {
    const point = mask.points.at(-1) ?? { x: .5, y: .5, pressure: 1 }
    const replaceLast = (patch: Partial<typeof point>) => ({ ...mask, points: [...mask.points.slice(0, -1), { ...point, ...patch }] }) as NativeMaskDefinition
    return <div className="mask-controls">
      {number('Radius', mask.radius, (radius) => ({ ...mask, radius }), { min: .001, max: 1 })}{number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 1 })}
      {number('Flow', mask.flow, (flow) => ({ ...mask, flow }), { min: 0, max: 1 })}{number('Point X', point.x, (x) => replaceLast({ x }), { min: 0, max: 1 })}
      {number('Point Y', point.y, (y) => replaceLast({ y }), { min: 0, max: 1 })}{number('Pressure', point.pressure, (pressure) => replaceLast({ pressure }), { min: 0, max: 1 })}
      <label><input aria-label="Brush erase" type="checkbox" checked={mask.erase} onChange={(event) => onChange({ ...mask, erase: event.target.checked })} /> Erase</label>
      <button type="button" onClick={() => onChange({ ...mask, points: [...mask.points, { x: .5, y: .5, pressure: 1 }] })}>+ Point</button>
      <button type="button" disabled={mask.points.length <= 1} onClick={() => onChange({ ...mask, points: mask.points.slice(0, -1) })}>Remove point</button>
    </div>
  }
  if (mask.type === 'luminance') return <div className="mask-controls">
    {number('Minimum', mask.minimum, (minimum) => ({ ...mask, minimum }), { min: 0, max: 16 })}{number('Maximum', mask.maximum, (maximum) => ({ ...mask, maximum }), { min: 0, max: 16 })}
    {number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 16 })}{invert(mask, (invert) => ({ ...mask, invert }))}
  </div>
  if (mask.type === 'colorRange') return <div className="mask-controls">
    {number('Red', mask.reference[0], (red) => ({ ...mask, reference: [red, mask.reference[1], mask.reference[2]] }), { min: 0, max: 16 })}
    {number('Green', mask.reference[1], (green) => ({ ...mask, reference: [mask.reference[0], green, mask.reference[2]] }), { min: 0, max: 16 })}
    {number('Blue', mask.reference[2], (blue) => ({ ...mask, reference: [mask.reference[0], mask.reference[1], blue] }), { min: 0, max: 16 })}
    {number('Tolerance', mask.tolerance, (tolerance) => ({ ...mask, tolerance }), { min: 0, max: 16 })}{number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 16 })}
    {invert(mask, (invert) => ({ ...mask, invert }))}
  </div>
  if (mask.type === 'portraitSemantic') return <div className="mask-controls portrait-mask-controls">
    <small>Local semantic cache: {mask.faceId.slice(0, 13)}…</small>
    {number('Threshold', mask.threshold, (threshold) => ({ ...mask, threshold }), { min: 0, max: 1, step: .01 })}
    {number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 1, step: .01 })}
    <small>{mask.region} · {mask.modelVersion.slice(0, 8)}</small>
  </div>
  if (mask.type === 'generated') return <div className="mask-controls portrait-mask-controls">
    <small>{mask.semanticClass} · {mask.metadata.executionProvider ?? mask.providerId}</small>
    {number('Threshold', mask.threshold, (threshold) => ({ ...mask, threshold }), { min: 0, max: 1, step: .01 })}
    {number('Feather', mask.feather, (feather) => ({ ...mask, feather }), { min: 0, max: 1, step: .01 })}
    {invert(mask, (invert) => ({ ...mask, invert }))}
    <small>{mask.modelVersion.slice(0, 12)} · {mask.modelHash.slice(0, 8)}</small>
  </div>
  return null
}
const takeSnapshot = (photo: PhotoItem): EditSnapshot => ({
  adjustments: { ...photo.adjustments }, curvePoints: copyCurve(photo.curvePoints), curveChannels: copyCurveChannels(photo.curveChannels), whiteBalanceMode: photo.whiteBalanceMode,
  whiteBalanceSample: photo.whiteBalanceSample ? { ...photo.whiteBalanceSample } : null, mask: { ...photo.mask },
  opticsState: { ...photo.opticsState, manualIdentity: photo.opticsState.manualIdentity ? { ...photo.opticsState.manualIdentity } : null },
  layers: copyLayers(photo.layers),
  skinRetouch: copySkinRetouch(photo.skinRetouch),
  healingOperations: copyHealingOperations(photo.healingOperations),
})
const applySnapshot = (photo: PhotoItem, snapshot: EditSnapshot) => ({
  ...photo, adjustments: { ...snapshot.adjustments }, curvePoints: copyCurve(snapshot.curvePoints), curveChannels: copyCurveChannels(snapshot.curveChannels),
  whiteBalanceMode: snapshot.whiteBalanceMode, whiteBalanceSample: snapshot.whiteBalanceSample ? { ...snapshot.whiteBalanceSample } : null, mask: { ...snapshot.mask },
  opticsState: { ...snapshot.opticsState, manualIdentity: snapshot.opticsState.manualIdentity ? { ...snapshot.opticsState.manualIdentity } : null },
  layers: copyLayers(snapshot.layers),
  skinRetouch: copySkinRetouch(snapshot.skinRetouch),
  healingOperations: copyHealingOperations(snapshot.healingOperations),
})
const hasCurveEdits = (points: ToneCurvePoint[]) => points.length !== defaultCurvePoints.length
  || points.some((point, index) => Math.abs(point.x - defaultCurvePoints[index].x) > .0001 || Math.abs(point.y - defaultCurvePoints[index].y) > .0001)
const hasMaskGeometryEdits = (mask: RadialMask) => (Object.keys(defaultMask) as Array<keyof RadialMask>)
  .some((key) => Math.abs(mask[key] - defaultMask[key]) > .0001)
const hasPhotoEdits = (photo: PhotoItem) => hasAdjustments(photo.adjustments) || hasCurveEdits(photo.curvePoints) || hasMaskGeometryEdits(photo.mask)
  || photo.opticsState.matchMode !== 'auto' || photo.opticsState.manualIdentity !== null || photo.layers.length > 0 || photo.skinRetouch.faces.length > 0 || photo.healingOperations.length > 0
const countPhotoEdits = (photo: PhotoItem) => (Object.keys(defaultAdjustments) as AdjustmentKey[])
  .filter((key) => photo.adjustments[key] !== defaultAdjustments[key]).length
  + (hasCurveEdits(photo.curvePoints) ? 1 : 0) + (hasMaskGeometryEdits(photo.mask) ? 1 : 0)
  + (photo.opticsState.matchMode !== 'auto' || photo.opticsState.manualIdentity ? 1 : 0)
  + photo.layers.length + (photo.skinRetouch.faces.length ? 1 : 0) + photo.healingOperations.length

const demoPhoto: PhotoItem = {
  id: 'starroom-demo',
  name: 'Starroom Demo.svg',
  src: '/starroom-demo.svg',
  renderBackend: 'browserFallback',
  imported: false,
  rating: 0,
  adjustments: { ...defaultAdjustments },
  curvePoints: copyCurve(defaultCurvePoints),
  curveChannels: defaultCurveChannels(),
  whiteBalanceMode: 'sourceDefault',
  whiteBalanceSample: null,
  opticsState: { ...defaultNativeOpticsState },
  mask: { ...defaultMask },
  layers: [],
  skinRetouch: defaultNativeSkinRetouch(),
  healingOperations: [],
  history: [],
  future: [],
}

const toolItems: Array<{ id: Tool; label: string; icon: typeof SunMedium }> = [
  { id: 'light', label: 'Light', icon: SunMedium },
  { id: 'color', label: 'Color', icon: Blend },
  { id: 'curve', label: 'Curve', icon: ScanLine },
  { id: 'detail', label: 'Detail', icon: Aperture },
  { id: 'looks', label: 'Looks', icon: Sparkles },
  { id: 'masks', label: 'Masks', icon: ScanFace },
  { id: 'heal', label: 'Heal', icon: Sparkles },
  { id: 'optics', label: 'Optics', icon: Contrast },
  { id: 'geometry', label: 'Geometry', icon: Crop },
]

const sliderGroups: Partial<Record<Tool, Array<{ key: AdjustmentKey; label: string; min: number; max: number; step: number; suffix?: string }>>> = {
  light: [
    { key: 'exposure', label: 'Exposure', min: -5, max: 5, step: .01, suffix: ' EV' },
    { key: 'contrast', label: 'Contrast', min: -100, max: 100, step: 1 },
    { key: 'highlights', label: 'Highlights', min: -100, max: 100, step: 1 },
    { key: 'shadows', label: 'Shadows', min: -100, max: 100, step: 1 },
    { key: 'whites', label: 'Whites', min: -100, max: 100, step: 1 },
    { key: 'blacks', label: 'Blacks', min: -100, max: 100, step: 1 },
  ],
  color: [
    { key: 'temperature', label: 'Temperature', min: -100, max: 100, step: 1 },
    { key: 'tint', label: 'Tint', min: -100, max: 100, step: 1 },
    { key: 'vibrance', label: 'Vibrance', min: -100, max: 100, step: 1 },
    { key: 'saturation', label: 'Saturation', min: -100, max: 100, step: 1 },
  ],
  detail: [
    { key: 'sharpness', label: 'Sharpen amount', min: 0, max: 100, step: 1 },
    { key: 'sharpenRadius', label: 'Sharpen radius', min: .3, max: 4, step: .1, suffix: ' px' },
    { key: 'sharpenDetail', label: 'Sharpen detail', min: 0, max: 100, step: 1 },
    { key: 'sharpenMasking', label: 'Sharpen masking', min: 0, max: 100, step: 1 },
    { key: 'sharpenHaloProtection', label: 'Halo protection', min: 0, max: 100, step: 1 },
    { key: 'texture', label: 'Texture', min: -100, max: 100, step: 1 },
    { key: 'clarity', label: 'Clarity', min: -100, max: 100, step: 1 },
    { key: 'dehaze', label: 'Dehaze', min: -100, max: 100, step: 1 },
    { key: 'denoiseLuminance', label: 'Denoise luminance', min: 0, max: 100, step: 1 },
    { key: 'denoiseChroma', label: 'Denoise chroma', min: 0, max: 100, step: 1 },
    { key: 'denoiseRadius', label: 'Denoise radius', min: .6, max: 4, step: .1, suffix: ' px' },
    { key: 'denoiseDetailProtection', label: 'Detail protection', min: 0, max: 100, step: 1 },
    { key: 'denoiseHighIso', label: 'High ISO', min: 0, max: 100, step: 1 },
    { key: 'aiDenoiseEnabled', label: 'AI Denoise enabled', min: 0, max: 1, step: 1 },
    { key: 'aiDenoiseAmount', label: 'AI Denoise amount', min: 0, max: 100, step: 1 },
    { key: 'aiDenoiseDetail', label: 'AI Detail preserve', min: 0, max: 100, step: 1 },
    { key: 'aiDenoiseColorNoise', label: 'AI Color noise', min: 0, max: 100, step: 1 },
    { key: 'aiDenoisePreserveSkin', label: 'AI Preserve skin', min: 0, max: 100, step: 1 },
  ],
  looks: [
    { key: 'grainAmount', label: 'Grain amount', min: 0, max: 100, step: 1 },
    { key: 'grainSize', label: 'Grain size', min: 10, max: 100, step: 1 },
    { key: 'grainRoughness', label: 'Grain roughness', min: 0, max: 100, step: 1 },
    { key: 'grainColor', label: 'Grain color', min: 0, max: 100, step: 1 },
    { key: 'vignette', label: 'Vignette amount', min: -100, max: 100, step: 1 },
    { key: 'vignetteMidpoint', label: 'Vignette midpoint', min: 0, max: 100, step: 1 },
    { key: 'vignetteRoundness', label: 'Vignette roundness', min: -100, max: 100, step: 1 },
    { key: 'vignetteFeather', label: 'Vignette feather', min: 2, max: 100, step: 1 },
    { key: 'vignetteHighlightProtect', label: 'Highlight protect', min: 0, max: 100, step: 1 },
  ],
  masks: [
    { key: 'maskExposure', label: 'Center exposure', min: -3, max: 3, step: .01, suffix: ' EV' },
    { key: 'maskFeather', label: 'Feather', min: 0, max: 100, step: 1 },
  ],
  geometry: [
    { key: 'geometryScale', label: 'Scale', min: 5, max: 200, step: .1, suffix: '%' },
    { key: 'geometryOffsetX', label: 'Offset X', min: -100, max: 100, step: .1, suffix: '%' },
    { key: 'geometryOffsetY', label: 'Offset Y', min: -100, max: 100, step: .1, suffix: '%' },
    { key: 'geometryVertical', label: 'Vertical perspective', min: -100, max: 100, step: .1 },
    { key: 'geometryHorizontal', label: 'Horizontal perspective', min: -100, max: 100, step: .1 },
    { key: 'cropLeft', label: 'Crop left', min: 0, max: 99, step: .1, suffix: '%' },
    { key: 'cropTop', label: 'Crop top', min: 0, max: 99, step: .1, suffix: '%' },
    { key: 'cropRight', label: 'Crop right', min: 1, max: 100, step: .1, suffix: '%' },
    { key: 'cropBottom', label: 'Crop bottom', min: 1, max: 100, step: .1, suffix: '%' },
    { key: 'rotation', label: 'Rotation', min: -180, max: 180, step: .1, suffix: '°' },
  ],
}

function usePersistedValue<T>(key: string, initial: T) {
  const [value, setValue] = useState<T>(() => {
    const saved = localStorage.getItem(key)
    return saved ? JSON.parse(saved) as T : initial
  })
  useEffect(() => localStorage.setItem(key, JSON.stringify(value)), [key, value])
  return [value, setValue] as const
}

function IconButton({ label, disabled, onClick, children }: { label: string; disabled?: boolean; onClick?: () => void; children: React.ReactNode }) {
  return <button className="icon-button" aria-label={label} title={label} disabled={disabled} onClick={onClick}>{children}</button>
}

function Slider({ label, value, min, max, step, suffix = '', onBeginEdit, onChange, onReset }: {
  label: string; value: number; min: number; max: number; step: number; suffix?: string
  onBeginEdit: () => void; onChange: (value: number) => void; onReset: () => void
}) {
  const [active, setActive] = useState(false)
  const percent = ((value - min) / (max - min)) * 100
  const display = step < 1 ? value.toFixed(step < .1 ? 2 : 1) : Math.round(value).toString()
  const [draft, setDraft] = useState(display)
  const [editing, setEditing] = useState(false)
  const commitDraft = () => {
    const parsed = Number(draft)
    if (Number.isFinite(parsed)) onChange(Math.min(max, Math.max(min, parsed)))
    setEditing(false)
  }
  return <div className="slider-row">
    <div className="slider-label"><span>{label}</span><label className="numeric-editor" title={`Type ${label} value`}>
      <input aria-label={`${label} value`} type="number" min={min} max={max} step={step} value={editing ? draft : display}
        onFocus={(event) => { onBeginEdit(); setEditing(true); setDraft(display); event.currentTarget.select() }}
        onChange={(event) => setDraft(event.target.value)} onBlur={commitDraft}
        onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); if (event.key === 'Escape') { setDraft(display); event.currentTarget.blur() } }} />
      {suffix && <span>{suffix.trim()}</span>}
    </label></div>
    <div className={`slider-wrap ${active ? 'is-active' : ''}`} style={{ '--fill': `${percent}%` } as React.CSSProperties}>
      <input aria-label={label} type="range" min={min} max={max} step={step} value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        onPointerDown={() => { onBeginEdit(); setActive(true) }} onPointerUp={() => setActive(false)}
        onBlur={() => setActive(false)} onDoubleClick={onReset} />
      <span className="value-bubble" style={{ left: `${percent}%` }}>{display}</span>
    </div>
  </div>
}

function Histogram({ values }: { values: number[] }) {
  return <div className="histogram" aria-label="Live photo histogram">
    {values.map((height, index) => <i key={index} style={{ height: `${Math.max(2, height * 100)}%` }} />)}
  </div>
}

function ToneCurveEditor({ points, selectedId, histogram, onSelect, onBeginEdit, onChange }: {
  points: ToneCurvePoint[]; selectedId: string | null
  histogram: number[]
  onSelect: (id: string) => void; onBeginEdit: () => void; onChange: (points: ToneCurvePoint[]) => void
}) {
  const svgRef = useRef<SVGSVGElement>(null)
  const [dragId, setDragId] = useState<string | null>(null)
  const sorted = [...points].sort((a, b) => a.x - b.x)
  const selected = sorted.find((point) => point.id === selectedId) ?? sorted[2] ?? sorted[0]
  const path = Array.from({ length: 61 }, (_, index) => {
    const x = index / 60
    const y = mapToneCurve(x, sorted)
    return `${index ? 'L' : 'M'} ${x * 300} ${(1 - y) * 120}`
  }).join(' ')
  const eventPoint = (event: React.PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    return {
      x: Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)),
      y: Math.max(0, Math.min(1, 1 - (event.clientY - rect.top) / rect.height)),
    }
  }
  const updatePoint = (id: string, next: Partial<ToneCurvePoint>) => {
    const updated = points.map((point) => point.id === id ? { ...point, ...next } : point).sort((a, b) => a.x - b.x)
    onChange(updated)
  }
  const addPoint = (event: React.PointerEvent<SVGSVGElement>) => {
    if (event.button !== 0 || event.target !== event.currentTarget && (event.target as Element).tagName !== 'path') return
    const position = eventPoint(event)
    const point = { id: crypto.randomUUID(), ...position }
    onBeginEdit()
    onChange([...points, point].sort((a, b) => a.x - b.x))
    onSelect(point.id)
  }
  const removePoint = (event: React.MouseEvent, id: string) => {
    event.preventDefault()
    if (id === 'black' || id === 'white') return
    onBeginEdit()
    onChange(points.filter((point) => point.id !== id))
    onSelect('midtone')
  }

  return <>
    <div className="curve-presets"><button onClick={() => { onBeginEdit(); onChange(copyCurve(defaultCurvePoints)) }}>Identity</button><button onClick={() => { onBeginEdit(); onChange([{ id: 'black', x: 0, y: 0 }, { id: 'shadow', x: .25, y: .18 }, { id: 'midtone', x: .5, y: .5 }, { id: 'highlight', x: .75, y: .84 }, { id: 'white', x: 1, y: 1 }]) }}>S-curve</button><button onClick={() => { onBeginEdit(); onChange([{ id: 'black', x: 0, y: .10 }, { id: 'midtone', x: .5, y: .55 }, { id: 'white', x: 1, y: 1 }]) }}>Black fade</button></div>
    <svg ref={svgRef} className="curve-preview curve-editor" viewBox="0 0 300 120" preserveAspectRatio="none"
      aria-label="Editable tone curve. Left click to add a point; drag points to adjust; right click a point to delete."
      onPointerDown={addPoint}
      onPointerMove={(event) => {
        if (!dragId) return
        const position = eventPoint(event)
        const endpoint = dragId === 'black' || dragId === 'white'
        updatePoint(dragId, { x: endpoint ? (dragId === 'black' ? 0 : 1) : position.x, y: position.y })
      }}
      onPointerUp={(event) => { setDragId(null); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}>
      <g className="curve-grid">
        <line x1="75" y1="0" x2="75" y2="120" /><line x1="150" y1="0" x2="150" y2="120" /><line x1="225" y1="0" x2="225" y2="120" />
        <line x1="0" y1="30" x2="300" y2="30" /><line x1="0" y1="60" x2="300" y2="60" /><line x1="0" y1="90" x2="300" y2="90" />
      </g>
      <g className="curve-histogram">{histogram.map((height, index) => <rect key={index} x={index * 300 / histogram.length} y={(1 - height) * 120} width={300 / histogram.length} height={height * 120} />)}</g>
      <line className="curve-baseline" x1="0" y1="120" x2="300" y2="0" />
      <path className="curve-hit-line" d={path} />
      <path className="curve-line" d={path} />
      {sorted.map((point) => <circle key={point.id} className={`curve-point ${point.id === selected?.id ? 'selected' : ''}`}
        cx={point.x * 300} cy={(1 - point.y) * 120} r="5"
        onPointerDown={(event) => { if (event.button !== 0) return; event.stopPropagation(); onSelect(point.id); onBeginEdit(); setDragId(point.id); svgRef.current?.setPointerCapture(event.pointerId) }}
        onContextMenu={(event) => removePoint(event, point.id)}>
        <title>Input {Math.round(point.x * 100)}, output {Math.round(point.y * 100)}{point.id === 'black' || point.id === 'white' ? ' (endpoint)' : ' · right click to delete'}</title>
      </circle>)}
    </svg>
    <div className="curve-help">Monotone curve · left click line to add · drag point · right click to delete</div>
    {selected && <div className="curve-values">
      <label>Input <input aria-label="Selected curve point input" type="number" min="0" max="100" step="1" value={Math.round(selected.x * 100)}
        disabled={selected.id === 'black' || selected.id === 'white'} onFocus={onBeginEdit}
        onChange={(event) => updatePoint(selected.id, { x: Number(event.target.value) / 100 })} /></label>
      <label>Output <input aria-label="Selected curve point output" type="number" min="0" max="100" step="1" value={Math.round(selected.y * 100)}
        onFocus={onBeginEdit} onChange={(event) => updatePoint(selected.id, { y: Number(event.target.value) / 100 })} /></label>
    </div>}
  </>
}

const libraryPhoto = (asset: NativeLibraryAsset, thumbnail: string): PhotoItem => ({
  id: `library-${asset.id}`,
  name: asset.sourcePath.split(/[\\/]/).pop() ?? asset.sourcePath,
  src: thumbnail,
  sourcePath: asset.sourcePath,
  renderBackend: 'native',
  imported: true,
  libraryAsset: asset,
  rating: asset.rating,
  adjustments: { ...defaultAdjustments },
  curvePoints: copyCurve(defaultCurvePoints),
  curveChannels: defaultCurveChannels(),
  whiteBalanceMode: 'sourceDefault',
  whiteBalanceSample: null,
  opticsState: { ...defaultNativeOpticsState },
  mask: { ...defaultMask },
  layers: [],
  skinRetouch: defaultNativeSkinRetouch(),
  healingOperations: [],
  history: [],
  future: [],
})

const applyNativeHistoryState = (photo: PhotoItem, state: NativeEditSettings): PhotoItem => {
  const mapped = fromNativeSettings(photo.adjustments, state)
  return { ...photo, adjustments: mapped.adjustments, curveChannels: mapped.curves, curvePoints: copyCurve(mapped.curves.master),
    whiteBalanceMode: state.whiteBalanceMode, whiteBalanceSample: state.whiteBalanceSample,
    opticsState: { matchMode: state.optics.matchMode, manualIdentity: state.optics.manualIdentity },
    layers: state.layers, skinRetouch: state.skinRetouch, healingOperations: state.healingOperations }
}

function CurveChannelTabs({ value, onChange }: { value: keyof NativeToneCurves; onChange: (value: keyof NativeToneCurves) => void }) {
  return <div className="curve-tabs" aria-label="Tone curve channel">{(['master', 'red', 'green', 'blue'] as const).map((channel) => <button key={channel}
    className={value === channel ? 'active' : ''} onClick={() => onChange(channel)}>{channel === 'master' ? 'Master' : channel[0].toUpperCase() + channel.slice(1)}</button>)}</div>
}

type MaskDragMode = 'move' | 'width' | 'height' | 'rotate' | null

function MaskOverlay({ bounds, mask, onBeginEdit, onChange }: {
  bounds: { left: number; top: number; width: number; height: number }
  mask: RadialMask; onBeginEdit: () => void; onChange: (mask: RadialMask) => void
}) {
  const [dragMode, setDragMode] = useState<MaskDragMode>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const position = (event: React.PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    return { x: (event.clientX - rect.left) / rect.width, y: (event.clientY - rect.top) / rect.height }
  }
  const angle = mask.rotation * Math.PI / 180
  const rotatePoint = (localX: number, localY: number) => ({
    x: mask.x + localX * Math.cos(angle) - localY * Math.sin(angle),
    y: mask.y + localX * Math.sin(angle) + localY * Math.cos(angle),
  })
  const widthHandle = rotatePoint(mask.width / 2, 0)
  const heightHandle = rotatePoint(0, mask.height / 2)
  const rotationHandle = rotatePoint(0, -mask.height / 2 - .08)
  const beginDrag = (event: React.PointerEvent, mode: MaskDragMode) => {
    if (event.button !== 0) return
    event.stopPropagation()
    onBeginEdit()
    setDragMode(mode)
    svgRef.current?.setPointerCapture(event.pointerId)
  }

  return <svg ref={svgRef} className="mask-overlay" style={{ left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height }}
    viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-label="Editable radial mask"
    onPointerDown={(event) => {
      if (event.button !== 0 || event.target !== event.currentTarget) return
      const next = position(event)
      onBeginEdit()
      onChange({ ...mask, x: Math.max(0, Math.min(1, next.x)), y: Math.max(0, Math.min(1, next.y)) })
    }}
    onPointerMove={(event) => {
      if (!dragMode) return
      const next = position(event)
      const dx = next.x - mask.x
      const dy = next.y - mask.y
      const localX = dx * Math.cos(-angle) - dy * Math.sin(-angle)
      const localY = dx * Math.sin(-angle) + dy * Math.cos(-angle)
      if (dragMode === 'move') onChange({ ...mask, x: Math.max(0, Math.min(1, next.x)), y: Math.max(0, Math.min(1, next.y)) })
      if (dragMode === 'width') onChange({ ...mask, width: Math.max(.04, Math.min(1.6, Math.abs(localX) * 2)) })
      if (dragMode === 'height') onChange({ ...mask, height: Math.max(.04, Math.min(1.6, Math.abs(localY) * 2)) })
      if (dragMode === 'rotate') onChange({ ...mask, rotation: Math.atan2(dy, dx) * 180 / Math.PI + 90 })
    }}
    onPointerUp={(event) => { setDragMode(null); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}>
    <g transform={`rotate(${mask.rotation} ${mask.x * 1000} ${mask.y * 1000})`}>
      <ellipse className="mask-feather-ring" cx={mask.x * 1000} cy={mask.y * 1000} rx={mask.width * 580} ry={mask.height * 580} />
      <ellipse className="mask-ring" cx={mask.x * 1000} cy={mask.y * 1000} rx={mask.width * 500} ry={mask.height * 500}
        onPointerDown={(event) => beginDrag(event, 'move')} />
    </g>
    <line className="mask-rotation-line" x1={mask.x * 1000} y1={mask.y * 1000} x2={rotationHandle.x * 1000} y2={rotationHandle.y * 1000} />
    <circle className="mask-center-handle" cx={mask.x * 1000} cy={mask.y * 1000} r="9" onPointerDown={(event) => beginDrag(event, 'move')} />
    <circle className="mask-handle" cx={widthHandle.x * 1000} cy={widthHandle.y * 1000} r="11" onPointerDown={(event) => beginDrag(event, 'width')} />
    <circle className="mask-handle" cx={heightHandle.x * 1000} cy={heightHandle.y * 1000} r="11" onPointerDown={(event) => beginDrag(event, 'height')} />
    <circle className="mask-rotate-handle" cx={rotationHandle.x * 1000} cy={rotationHandle.y * 1000} r="12" onPointerDown={(event) => beginDrag(event, 'rotate')} />
  </svg>
}

function FourPointOverlay({ values, onBeginEdit, onAdjust }: {
  values: Adjustments; onBeginEdit: () => void
  onAdjust: (key: AdjustmentKey, value: number, recordHistory?: boolean) => void
}) {
  const [drag, setDrag] = useState<{ x: AdjustmentKey; y: AdjustmentKey } | null>(null)
  const handles: Array<[AdjustmentKey, AdjustmentKey]> = [
    ['quadTopLeftX', 'quadTopLeftY'], ['quadTopRightX', 'quadTopRightY'],
    ['quadBottomRightX', 'quadBottomRightY'], ['quadBottomLeftX', 'quadBottomLeftY'],
  ]
  const move = (event: React.PointerEvent<SVGSVGElement>) => {
    if (!drag) return
    const rect = event.currentTarget.getBoundingClientRect()
    onAdjust(drag.x, Math.max(0, Math.min(100, (event.clientX - rect.left) / rect.width * 100)), false)
    onAdjust(drag.y, Math.max(0, Math.min(100, (event.clientY - rect.top) / rect.height * 100)), false)
  }
  return <svg className="quad-overlay" viewBox="0 0 100 100" preserveAspectRatio="none"
    aria-label="Draggable four-point perspective guides" onPointerMove={move}
    onPointerUp={(event) => { setDrag(null); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}>
    <polygon points={`${values.quadTopLeftX},${values.quadTopLeftY} ${values.quadTopRightX},${values.quadTopRightY} ${values.quadBottomRightX},${values.quadBottomRightY} ${values.quadBottomLeftX},${values.quadBottomLeftY}`} />
    {handles.map(([x, y]) => <circle key={x} cx={values[x]} cy={values[y]} r="1.8"
      onPointerDown={(event) => { if (event.button !== 0) return; event.stopPropagation(); onBeginEdit(); setDrag({ x, y }); event.currentTarget.ownerSVGElement?.setPointerCapture(event.pointerId) }} />)}
  </svg>
}

function PreviewCanvas({ photo, before, zoom, maskActive = false, healActive = false, brushActive = false, maskPreview = null, onBeginMaskEdit, onMaskChange, onHealingStroke, onBrushStroke, onWhiteBalancePick, onColorSample, onHistogram, onStatus, onDimensions, metric = true }: {
  photo: PhotoItem; before: boolean; zoom: 'fit' | '100'
  maskActive?: boolean; onBeginMaskEdit?: () => void; onMaskChange?: (mask: RadialMask) => void
  healActive?: boolean; onHealingStroke?: (points: Array<{ x: number; y: number }>) => void
  brushActive?: boolean; onBrushStroke?: (points: Array<{ x: number; y: number }>) => void
  maskPreview?: NativeMaskTree | null
  onWhiteBalancePick?: (sample: NativeWhiteBalanceSample) => void
  onColorSample?: (x: number, y: number) => void
  onHistogram: (values: number[]) => void
  onStatus: (status: string) => void
  onDimensions: (dimensions: string) => void
  metric?: boolean
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [canvasBounds, setCanvasBounds] = useState({ left: 0, top: 0, width: 0, height: 0 })
  const healingStroke = useRef<Array<{ x: number; y: number }> | null>(null)
  const maskBrushStroke = useRef<Array<{ x: number; y: number }> | null>(null)
  const healPoint = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect()
    return { x: Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)), y: Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height)) }
  }

  useEffect(() => {
    let cancelled = false
    const timeout = window.setTimeout(async () => {
      onStatus('Rendering…')
      try {
        const adjustments = before ? defaultAdjustments : photo.adjustments
        const curvePoints = before ? defaultCurvePoints : photo.curvePoints
        const mask = before ? defaultMask : photo.mask
        let rendered: CanvasImageSource
        let renderedWidth: number
        let renderedHeight: number
        let nativeProfile = ''
        let nativeAcceleration: 'gpu' | 'cpuFallback' = 'cpuFallback'
        let release: (() => void) | undefined
        if (photo.renderBackend === 'native') {
          if (!photo.sourcePath) throw new Error('Native photo is missing its source path; Browser fallback was not used.')
          const previewLayers = maskPreview && !before ? [
            ...photo.layers,
            { ...defaultLayer(), id: '__mask-preview-dim__', name: 'Mask preview outside', opacity: .72, mask: { operation: 'invert' as const, children: [structuredClone(maskPreview)] }, adjustments: { tone: { ...defaultLayer().adjustments.tone, exposureEv: -1.35 } } },
            { ...defaultLayer(), id: '__mask-preview-inside__', name: 'Mask preview inside', opacity: .38, mask: structuredClone(maskPreview), adjustments: { tone: { ...defaultLayer().adjustments.tone, exposureEv: .55 } } },
          ] : photo.layers
          const result = await renderNativePreview(photo.sourcePath, adjustments, curvePoints, mask,
            before ? 'sourceDefault' : photo.whiteBalanceMode, before ? null : photo.whiteBalanceSample,
            before ? defaultCurveChannels() : photo.curveChannels, before ? defaultNativeOpticsState : photo.opticsState,
            before ? [] : previewLayers, 1800, before ? defaultNativeSkinRetouch() : photo.skinRetouch, before ? [] : photo.healingOperations)
          const jpegBuffer = result.jpeg.buffer.slice(
            result.jpeg.byteOffset,
            result.jpeg.byteOffset + result.jpeg.byteLength,
          ) as ArrayBuffer
          const blobUrl = URL.createObjectURL(new Blob([jpegBuffer], { type: 'image/jpeg' }))
          release = () => URL.revokeObjectURL(blobUrl)
          const image = new Image()
          await new Promise<void>((resolve, reject) => {
            image.onload = () => resolve()
            image.onerror = () => reject(new Error('Native preview JPEG could not be decoded.'))
            image.src = blobUrl
          })
          rendered = image
          renderedWidth = result.width
          renderedHeight = result.height
          nativeProfile = result.cameraProfileId ?? result.inputProfile
          nativeAcceleration = result.acceleration
        } else {
          const fallback = await renderImageSource(photo.src, adjustments, 1800, curvePoints, mask)
          rendered = fallback
          renderedWidth = fallback.width
          renderedHeight = fallback.height
        }
        if (cancelled || !canvasRef.current) {
          release?.()
          return
        }
        const canvas = canvasRef.current
        canvas.width = renderedWidth
        canvas.height = renderedHeight
        const context = canvas.getContext('2d', { willReadFrequently: true })
        if (!context) {
          release?.()
          throw new Error('Canvas 2D is unavailable.')
        }
        context.drawImage(rendered, 0, 0)
        release?.()
        window.requestAnimationFrame(() => {
          if (!canvasRef.current) return
          setCanvasBounds({ left: canvasRef.current.offsetLeft, top: canvasRef.current.offsetTop,
            width: canvasRef.current.clientWidth, height: canvasRef.current.clientHeight })
        })
        if (metric) {
          onHistogram(calculateHistogram(context.getImageData(0, 0, canvas.width, canvas.height)))
          onDimensions(`${renderedWidth} × ${renderedHeight}`)
          onStatus(photo.renderBackend === 'native'
            ? `${nativeAcceleration === 'gpu' ? 'Native GPU' : 'Native CPU fallback'} · ${nativeProfile}${before ? ' · original' : ''}`
            : `Browser fallback${before ? ' · original' : ''}`)
        }
      } catch (error) {
        if (!cancelled) onStatus(error instanceof Error ? error.message : 'Preview failed')
      }
    }, 30)

    return () => {
      cancelled = true
      window.clearTimeout(timeout)
    }
  }, [before, metric, onDimensions, onHistogram, onStatus, photo.adjustments, photo.curvePoints, photo.curveChannels, photo.whiteBalanceMode, photo.whiteBalanceSample,
    photo.mask, photo.opticsState, photo.layers, photo.skinRetouch, photo.healingOperations, photo.renderBackend, photo.sourcePath, photo.src, maskPreview])

  useEffect(() => {
    const measure = () => canvasRef.current && setCanvasBounds({ left: canvasRef.current.offsetLeft, top: canvasRef.current.offsetTop,
      width: canvasRef.current.clientWidth, height: canvasRef.current.clientHeight })
    window.addEventListener('resize', measure)
    return () => window.removeEventListener('resize', measure)
  }, [])

  return <>
    <canvas ref={canvasRef} className={`photo-canvas zoom-${zoom}`} aria-label={`Edited preview of ${photo.name}`}
      onPointerDown={(event) => { if (before || event.button !== 0 || (!healActive && !brushActive)) return; const points = [healPoint(event)]; if (healActive) healingStroke.current = points; else maskBrushStroke.current = points; event.currentTarget.setPointerCapture(event.pointerId) }}
      onPointerMove={(event) => { const points = healingStroke.current ?? maskBrushStroke.current; if (!points) return; const point = healPoint(event); const previous = points.at(-1)!; if (Math.hypot(point.x - previous.x, point.y - previous.y) >= .004) points.push(point) }}
      onPointerUp={(event) => { const healing = healingStroke.current; const brushing = maskBrushStroke.current; healingStroke.current = null; maskBrushStroke.current = null; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); if (healing?.length) onHealingStroke?.(healing); if (brushing?.length) onBrushStroke?.(brushing) }}
      onDoubleClick={(event) => {
        if (before) return
        const bounds = event.currentTarget.getBoundingClientRect()
        const pointX = Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width))
        const pointY = Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height))
        if (onColorSample) { onColorSample(pointX, pointY); return }
        if (photo.whiteBalanceMode !== 'neutralPicker' || !onWhiteBalancePick) return
        const size = .06
        const x = Math.max(0, Math.min(1 - size, pointX - size / 2))
        const y = Math.max(0, Math.min(1 - size, pointY - size / 2))
        onWhiteBalancePick({ x, y, width: size, height: size })
      }} />
    {maskActive && canvasBounds.width > 0 && onBeginMaskEdit && onMaskChange
      ? <MaskOverlay bounds={canvasBounds} mask={photo.mask} onBeginEdit={onBeginMaskEdit} onChange={onMaskChange} /> : null}
  </>
}

function Inspector({ tool, values, curvePoints, curveChannel, histogram, onCurveChannel, selectedCurvePoint, mask, renderBackend, whiteBalanceMode, onAdjust, onBeginAdjustment, onReset,
  onCurveSelect, onCurveBegin, onCurveChange, onCurvePresetSave, onCurvePresetLoad, canLoadCurvePreset, onMaskBegin, onMaskChange, onWhiteBalanceMode, onCopyWhiteBalance, onPasteWhiteBalance,
  mixerBand, onMixerBand, mixerPicking, onMixerPicking, opticsState, opticsStatus, onOpticsState, onResolveOptics }: {
  tool: Tool; values: Adjustments; curvePoints: ToneCurvePoint[]; curveChannel: keyof NativeToneCurves; histogram: number[]; onCurveChannel: (channel: keyof NativeToneCurves) => void; selectedCurvePoint: string | null; mask: RadialMask; renderBackend: RenderBackend
  onAdjust: (key: AdjustmentKey, value: number, recordHistory?: boolean) => void
  onBeginAdjustment: () => void
  onReset: (key: AdjustmentKey) => void
  onCurveSelect: (id: string) => void; onCurveBegin: () => void; onCurveChange: (points: ToneCurvePoint[]) => void
  onCurvePresetSave: () => void; onCurvePresetLoad: () => void; canLoadCurvePreset: boolean
  onMaskBegin: () => void; onMaskChange: (mask: RadialMask) => void
  whiteBalanceMode: NativeWhiteBalanceMode; onWhiteBalanceMode: (mode: NativeWhiteBalanceMode) => void
  onCopyWhiteBalance: () => void; onPasteWhiteBalance: () => void
  mixerBand: string; onMixerBand: (band: string) => void; mixerPicking: boolean; onMixerPicking: () => void
  opticsState: NativeOpticsState; opticsStatus: NativeLensProfileResolution | null
  onOpticsState: (state: NativeOpticsState) => void; onResolveOptics: () => void
}) {
  const mixerBands = ['Red', 'Orange', 'Yellow', 'Green', 'Cyan', 'Blue', 'Purple', 'Magenta'] as const
  const gradingZones = ['Global', 'Shadows', 'Midtones', 'Highlights'] as const
  const [gradingZone, setGradingZone] = useState<(typeof gradingZones)[number]>('Global')
  const sliders = sliderGroups[tool] ?? []
  const normalizeAngle = (value: number) => ((value + 180) % 360 + 360) % 360 - 180
  return <section className="inspector-content" aria-label={`${tool} inspector`}>
    <div className="inspector-head"><div><span className="eyebrow">Live CPU preview</span><h2>{tool}</h2></div><ChevronDown size={16} /></div>
    {renderBackend === 'native' && tool === 'masks'
      && <div className="tool-note">Native M15 mask layer: the dashed radial selection is evaluated in the shared Preview/Before-After/Export graph. No Browser Canvas compositing is used.</div>}
    {tool === 'color' && <>
      <div className="tool-note">Encoded-image Temperature/Tint are relative corrections, not physical Kelvin. RAW Camera/As-Shot uses LibRaw metadata.</div>
      {renderBackend === 'native' && <div className="wb-controls"><label>White balance mode<select value={whiteBalanceMode}
        onFocus={onBeginAdjustment} onChange={(event) => onWhiteBalanceMode(event.target.value as NativeWhiteBalanceMode)}>
        <option value="sourceDefault">Source default</option><option value="asShot">As Shot (RAW)</option>
        <option value="camera">Camera (RAW)</option><option value="auto">Auto (gray world)</option>
        <option value="neutralPicker">Neutral picker</option><option value="relative">Relative (encoded)</option>
      </select></label><div><button onClick={onCopyWhiteBalance}>Copy WB</button><button onClick={onPasteWhiteBalance}>Paste WB</button></div>
      <small>{whiteBalanceMode === 'neutralPicker' ? 'Double-click a neutral area in the preview to sample it.' : 'Mode is recorded with this non-destructive edit.'}</small></div>}
      <div className="mixer-panel" aria-label="Eight-band Color Mixer">
        <div className="mixer-heading"><strong>Color Mixer</strong><button className={mixerPicking ? 'active' : ''} onClick={onMixerPicking}>Target</button><label><input type="checkbox" checked={values.mixerHueLock !== 0}
          onFocus={onBeginAdjustment} onChange={(event) => onAdjust('mixerHueLock', event.target.checked ? 1 : 0)} /> Hue lock</label></div>
        <div className="mixer-tabs" role="tablist" aria-label="Color Mixer bands">
          {mixerBands.map((band) => <button key={band} role="tab" aria-selected={band === mixerBand}
            className={band === mixerBand ? `active band-${band.toLowerCase()}` : `band-${band.toLowerCase()}`}
            onClick={() => onMixerBand(band)}>{band}</button>)}
        </div>
        {([['Hue', -30, 30, 1, '°'], ['Chroma', -100, 100, 1, ''], ['Lightness', -100, 100, 1, '']] as const)
          .map(([control, min, max, step, suffix]) => {
            const key = `mixer${mixerBand}${control}` as AdjustmentKey
            return <Slider key={key} label={`${mixerBand} ${control}`} value={values[key]} min={min} max={max} step={step} suffix={suffix}
              onBeginEdit={onBeginAdjustment} onChange={(value) => onAdjust(key, value, false)} onReset={() => onReset(key)} />
          })}
        <small>Targeted edits are calculated in native OKLCh with circular, overlapping hue bands.</small>
      </div>
      <div className="grading-panel" aria-label="Four-way Color Grading">
        <div className="mixer-heading"><strong>Color Grading</strong><small>Native OKLab</small></div>
        <div className="grading-tabs" role="tablist" aria-label="Color grading tonal zones">
          {gradingZones.map((zone) => <button key={zone} role="tab" aria-selected={zone === gradingZone}
            className={zone === gradingZone ? 'active' : ''} onClick={() => setGradingZone(zone)}>{zone}</button>)}
        </div>
        {([['Hue', -180, 180, 1, '°'], ['Chroma', -100, 100, 1, ''], ['Lightness', -100, 100, 1, '']] as const)
          .map(([control, min, max, step, suffix]) => {
            const key = `grade${gradingZone}${control}` as AdjustmentKey
            return <Slider key={key} label={`${gradingZone} ${control}`} value={values[key]} min={min} max={max} step={step} suffix={suffix}
              onBeginEdit={onBeginAdjustment} onChange={(value) => onAdjust(key, value, false)} onReset={() => onReset(key)} />
          })}
        {([['Balance', 'gradeBalance'], ['Blending', 'gradeBlending'], ['Amount', 'gradeAmount']] as const)
          .map(([label, key]) => <Slider key={key} label={label} value={values[key]} min={label === 'Balance' ? -100 : 0} max={100} step={1}
            onBeginEdit={onBeginAdjustment} onChange={(value) => onAdjust(key, value, false)} onReset={() => onReset(key)} />)}
      </div>
    </>}
    {tool === 'masks' && <div className="tool-note">Click the photo to place the mask. Drag inside to move; drag side handles to resize; drag the top handle to rotate.</div>}
    {tool === 'curve' && <><CurveChannelTabs value={curveChannel} onChange={onCurveChannel} /><ToneCurveEditor points={curvePoints} selectedId={selectedCurvePoint} onSelect={onCurveSelect}
      histogram={histogram} onBeginEdit={onCurveBegin} onChange={onCurveChange} /><div className="curve-presets"><button onClick={onCurvePresetSave}>Save custom</button><button disabled={!canLoadCurvePreset} onClick={onCurvePresetLoad}>Load custom</button></div></>}
    {tool === 'optics' && <div className="optics-controls">
      <div className="tool-note">Lensfun v0.3.4 profile correction. Missing, ambiguous or mismatched metadata is reported explicitly.</div>
      <label><input type="checkbox" checked={values.lensCorrection !== 0} onChange={(event) => onAdjust('lensCorrection', event.target.checked ? 1 : 0)} /> Enable Lensfun correction</label>
      <label><input type="checkbox" checked={values.lensDistortion !== 0} onChange={(event) => onAdjust('lensDistortion', event.target.checked ? 1 : 0)} /> Distortion</label>
      <label><input type="checkbox" checked={values.lensTca !== 0} onChange={(event) => onAdjust('lensTca', event.target.checked ? 1 : 0)} /> TCA</label>
      <label><input type="checkbox" checked={values.lensVignette !== 0} onChange={(event) => onAdjust('lensVignette', event.target.checked ? 1 : 0)} /> Vignette</label>
      <label><input type="checkbox" checked={values.lensAutoScale !== 0} onChange={(event) => onAdjust('lensAutoScale', event.target.checked ? 1 : 0)} /> Auto scale</label>
      <label>Match mode<select value={opticsState.matchMode} onChange={(event) => onOpticsState({ ...opticsState, matchMode: event.target.value as 'auto' | 'manual',
        manualIdentity: event.target.value === 'manual' ? opticsState.manualIdentity ?? { cameraMake: '', cameraModel: '', lensMake: '', lensModel: '', focalLengthMm: 0, aperture: 0, focusDistanceM: null } : null })}>
        <option value="auto">Auto metadata</option><option value="manual">Manual profile</option></select></label>
      {opticsState.matchMode === 'manual' && <div className="optics-manual">{([
        ['Camera make', 'cameraMake'], ['Camera model', 'cameraModel'], ['Lens make', 'lensMake'], ['Lens model', 'lensModel'],
      ] as const).map(([label, key]) => <label key={key}>{label}<input value={opticsState.manualIdentity?.[key] ?? ''}
        onChange={(event) => onOpticsState({ ...opticsState, manualIdentity: { ...(opticsState.manualIdentity as NativeLensIdentity), [key]: event.target.value } })} /></label>)}
        {([['Focal mm', 'focalLengthMm'], ['Aperture', 'aperture'], ['Focus m', 'focusDistanceM']] as const).map(([label, key]) => <label key={key}>{label}<input type="number" min="0" step="0.1"
          value={opticsState.manualIdentity?.[key] ?? ''} onChange={(event) => onOpticsState({ ...opticsState,
            manualIdentity: { ...(opticsState.manualIdentity as NativeLensIdentity), [key]: event.target.value === '' ? (key === 'focusDistanceM' ? null : 0) : Number(event.target.value) } })} /></label>)}</div>}
      <button onClick={onResolveOptics}>Resolve Lensfun profile</button>
      <div className={`optics-status status-${opticsStatus?.status ?? 'idle'}`}><strong>{opticsStatus?.status ?? 'Not resolved'}</strong>
        <span>{opticsStatus?.profileId ?? 'No profile selected'}</span><small>{opticsStatus?.cameraMount ?? ''} · DB {opticsStatus?.databaseVersion ?? '0.3.4'}</small></div>
    </div>}
    {sliders.map(({ key, ...slider }) => <Slider key={key} {...slider} value={values[key]} onBeginEdit={onBeginAdjustment}
      onChange={(value) => onAdjust(key, value, false)} onReset={() => onReset(key)} />)}
    {tool === 'masks' && <div className="mask-values">
      {([
        ['Center X', 'x', mask.x * 100, 0, 100, '%'], ['Center Y', 'y', mask.y * 100, 0, 100, '%'],
        ['Width', 'width', mask.width * 100, 4, 160, '%'], ['Height', 'height', mask.height * 100, 4, 160, '%'],
        ['Angle', 'rotation', mask.rotation, -180, 180, '°'],
      ] as const).map(([label, key, value, min, max, suffix]) => <label key={key}>{label}<span><input aria-label={`Mask ${label} value`} type="number"
        min={min} max={max} step={key === 'rotation' ? .1 : 1} value={Math.round(value * 10) / 10}
        onFocus={onMaskBegin} onChange={(event) => {
          const next = Math.min(max, Math.max(min, Number(event.target.value)))
          onMaskChange({ ...mask, [key]: key === 'rotation' ? next : next / 100 })
        }} />{suffix}</span></label>)}
    </div>}
    {tool === 'geometry' && <div className="geometry-controls">
      <label>Upright<select value={Math.round(values.geometryUpright)} onChange={(event) => onAdjust('geometryUpright', Number(event.target.value))}>
        <option value="0">Off</option><option value="1">Auto</option><option value="2">Level</option><option value="3">Vertical</option><option value="4">Full</option></select></label>
      <div className="aspect-presets"><button onClick={() => { onBeginAdjustment(); onAdjust('cropAspectWidth', 0, false); onAdjust('cropAspectHeight', 0, false) }}>Free</button>
        <button onClick={() => { onBeginAdjustment(); onAdjust('cropAspectWidth', -1, false); onAdjust('cropAspectHeight', -1, false) }}>Original</button>
        {([[1,1,'1:1'],[4,3,'4:3'],[3,2,'3:2'],[16,9,'16:9']] as const).map(([width,height,label]) => <button key={label}
          onClick={() => { onBeginAdjustment(); onAdjust('cropAspectWidth', width, false); onAdjust('cropAspectHeight', height, false) }}>{label}</button>)}</div>
      <button onClick={() => onAdjust('rotation', normalizeAngle(values.rotation - 90))}><RotateCcw size={16} /> Rotate left</button>
      <button onClick={() => onAdjust('rotation', normalizeAngle(values.rotation + 90))}><RotateCw size={16} /> Rotate right</button>
      <button className={values.flipHorizontal ? 'active' : ''} onClick={() => onAdjust('flipHorizontal', values.flipHorizontal ? 0 : 1)}><FlipHorizontal2 size={16} /> Flip horizontal</button>
      <button className={values.flipVertical ? 'active' : ''} onClick={() => onAdjust('flipVertical', values.flipVertical ? 0 : 1)}><FlipVertical2 size={16} /> Flip vertical</button>
      <button className={values.geometryFourPoint ? 'active' : ''} onClick={() => onAdjust('geometryFourPoint', values.geometryFourPoint ? 0 : 1)}>Four-point perspective</button>
      {values.geometryFourPoint !== 0 && <div className="quad-values">{([
        ['TL X','quadTopLeftX'],['TL Y','quadTopLeftY'],['TR X','quadTopRightX'],['TR Y','quadTopRightY'],
        ['BR X','quadBottomRightX'],['BR Y','quadBottomRightY'],['BL X','quadBottomLeftX'],['BL Y','quadBottomLeftY'],
      ] as const).map(([label,key]) => <label key={key}>{label}<input type="number" min="0" max="100" step="0.1" value={values[key]}
        onFocus={onBeginAdjustment} onChange={(event) => onAdjust(key, Math.min(100, Math.max(0, Number(event.target.value))), false)} />%</label>)}</div>}
    </div>}
    <div className="intent-card"><Sparkles size={17} /><div><strong>Non-destructive edits</strong><span>Type values directly or double-click a slider to reset</span></div></div>
  </section>
}

function AppHeader({ view, setView, theme, setTheme, before, setBefore, canUndo, canRedo, undo, redo, onExport }: {
  view: WorkspaceView; setView: (view: WorkspaceView) => void
  theme: Theme; setTheme: (theme: Theme) => void
  before: boolean; setBefore: (value: boolean) => void; canUndo: boolean; canRedo: boolean
  undo: () => void; redo: () => void; onExport: () => void
}) {
  return <header className="topbar">
    <div className="brand"><span className="brand-mark"><Aperture size={18} /></span><strong>Starroom</strong></div>
    <nav aria-label="Workspace">
      <button className={view === 'library' ? 'active' : ''} onClick={() => setView('library')}>Library</button>
      <button className={view === 'edit' ? 'active' : ''} onClick={() => setView('edit')}>Edit</button>
      <button className={view === 'compare' ? 'active' : ''} onClick={() => setView('compare')}>Compare</button>
    </nav>
    <div className="top-actions">
      <button className={before ? 'text-button active' : 'text-button'} onClick={() => setBefore(!before)}><Columns2 size={15} /> Before</button>
      <IconButton label="Undo" disabled={!canUndo} onClick={undo}><Undo2 size={17} /></IconButton>
      <IconButton label="Redo" disabled={!canRedo} onClick={redo}><Redo2 size={17} /></IconButton>
      <select aria-label="Theme" value={theme} onChange={(event) => setTheme(event.target.value as Theme)}><option value="dark">Dark</option><option value="gray">Gray</option><option value="light">Light</option></select>
      <button className="export-button" onClick={onExport}><Download size={15} /> Export JPEG</button>
    </div>
  </header>
}

export function App() {
  const [theme, setTheme] = usePersistedValue<Theme>('starroom-theme', 'dark')
  const [leftOpen, setLeftOpen] = usePersistedValue('starroom-left-panel', true)
  const [filmstripOpen, setFilmstripOpen] = usePersistedValue('starroom-filmstrip', true)
  const [photos, setPhotos] = useState<PhotoItem[]>([demoPhoto])
  const [selectedId, setSelectedId] = useState(demoPhoto.id)
  const [filter, setFilter] = useState<LibraryFilter>('all')
  const [view, setView] = useState<WorkspaceView>('edit')
  const [tool, setTool] = useState<Tool>('light')
  const [selectedCurvePoint, setSelectedCurvePoint] = useState<string | null>('midtone')
  const [curveChannel, setCurveChannel] = useState<keyof NativeToneCurves>('master')
  const [before, setBefore] = useState(false)
  const [zoom, setZoom] = useState<'fit' | '100'>('fit')
  const [zoomScale, setZoomScale] = useState(1)
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const panStart = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null)
  const [histogram, setHistogram] = useState(() => Array.from({ length: 48 }, () => 0))
  const [renderStatus, setRenderStatus] = useState('Ready')
  const [dimensions, setDimensions] = useState('—')
  const [notice, setNotice] = useState('')
  const [copiedWhiteBalance, setCopiedWhiteBalance] = useState<Pick<PhotoItem, 'whiteBalanceMode' | 'whiteBalanceSample'> | null>(null)
  const [savedCurvePreset, setSavedCurvePreset] = usePersistedValue<NativeToneCurves | null>('starroom-custom-curve-preset', null)
  const [mixerBand, setMixerBand] = useState('Red')
  const [mixerPicking, setMixerPicking] = useState(false)
  const [opticsStatus, setOpticsStatus] = useState<NativeLensProfileResolution | null>(null)
  const [selectedLayerId, setSelectedLayerId] = useState<string | null>(null)
  const [portraitDetection, setPortraitDetection] = useState<NativePortraitDetection | null>(null)
  const [portraitFaceId, setPortraitFaceId] = useState<string | null>(null)
  const [advisorResult, setAdvisorResult] = useState<NativeAdvisorResult | null>(null)
  const [advisorPreview, setAdvisorPreview] = useState<EditSnapshot | null>(null)
  const [aiMaskResult, setAiMaskResult] = useState<NativeAiMaskResult | null>(null)
  const [aiMaskRequestId, setAiMaskRequestId] = useState<string | null>(null)
  const [maskOverlayVisible, setMaskOverlayVisible] = useState(false)
  const [lookAmount, setLookAmount] = usePersistedValue('starroom-look-amount', 100)
  const [referencePath, setReferencePath] = useState<string | null>(null)
  const [referenceResult, setReferenceResult] = useState<NativeReferenceMatchResponse | null>(null)
  const [referenceBase, setReferenceBase] = useState<NativeEditSettings | null>(null)
  const [referenceControls, setReferenceControls] = useState({ amount: 70, tone: 100, color: 100, grading: 100, protectSkin: 80 })
  const [lookAPath, setLookAPath] = useState<string | null>(null)
  const [lookBPath, setLookBPath] = useState<string | null>(null)
  const [lookAWeight, setLookAWeight] = useState(70)
  const [lookBWeight, setLookBWeight] = useState(30)
  const [libraryAssets, setLibraryAssets] = useState<NativeLibraryAsset[]>([])
  const [selectedLibraryIds, setSelectedLibraryIds] = useState<number[]>([])
  const [librarySearch, setLibrarySearch] = useState('')
  const [libraryBusy, setLibraryBusy] = useState(false)
  const libraryAnchor = useRef<number | null>(null)
  const [nativeHistory, setNativeHistory] = useState<NativeHistoryResult | null>(null)
  const [snapshotName, setSnapshotName] = useState('Version 1')
  const openedHistoryAsset = useRef<number | null>(null)
  const pendingNativeBefore = useRef<NativeEditSettings | null>(null)
  const nativeHistoryTimer = useRef<number | null>(null)
  const applyingNativeHistory = useRef(false)
  const fileInput = useRef<HTMLInputElement>(null)
  const objectUrls = useRef(new Set<string>())

  useEffect(() => () => objectUrls.current.forEach((url) => URL.revokeObjectURL(url)), [])
  useEffect(() => {
    if (!nativeRuntimeAvailable()) return
    let active = true
    void (async () => {
      try {
        await openNativeLibrary()
        const assets = await queryNativeLibrary({ limit: 200 })
        const thumbnails = await Promise.all(assets.map(async (asset) => {
          try { return await nativeLibraryThumbnail(asset.id) } catch { return '' }
        }))
        if (!active) return
        setLibraryAssets(assets)
        const libraryPhotos = assets.map((asset, index) => libraryPhoto(asset, thumbnails[index]))
        if (libraryPhotos.length) {
          setPhotos((current) => [...libraryPhotos, ...current.filter((photo) => !photo.libraryAsset)])
          setSelectedLibraryIds([assets[0].id])
        }
      } catch (error) {
        if (active) setNotice(error instanceof Error ? error.message : 'Library initialization failed')
      }
    })()
    return () => { active = false }
  }, [])
  useEffect(() => {
    if (!notice) return
    const timeout = window.setTimeout(() => setNotice(''), 3500)
    return () => window.clearTimeout(timeout)
  }, [notice])

  function selectPhoto(id: string) {
    setSelectedId(id)
    setZoom('fit')
    setReferenceResult(null)
    setReferenceBase(null)
    setZoomScale(1)
    setPan({ x: 0, y: 0 })
    setOpticsStatus(null)
    setPortraitDetection(null)
    setPortraitFaceId(null)
    setAdvisorResult(null)
    setAdvisorPreview(null)
    setAiMaskResult(null)
    setAiMaskRequestId(null)
    setMaskOverlayVisible(false)
  }

  const selected = photos.find((photo) => photo.id === selectedId) ?? photos[0]
  const nativeHistoryState = useMemo(() => toNativeSettings(
    selected.adjustments, selected.curvePoints, selected.whiteBalanceMode, selected.whiteBalanceSample,
    selected.curveChannels, selected.opticsState, selected.layers, selected.mask,
    selected.skinRetouch, selected.healingOperations,
  ), [selected])

  useEffect(() => {
    const assetId = selected.libraryAsset?.id
    if (!assetId || openedHistoryAsset.current === assetId) return
    openedHistoryAsset.current = assetId
    void openNativeHistory(assetId, nativeHistoryState).then((result) => {
      applyingNativeHistory.current = true
      setNativeHistory(result)
      setPhotos((current) => current.map((photo) => photo.libraryAsset?.id === assetId ? applyNativeHistoryState(photo, result.state) : photo))
      window.setTimeout(() => { applyingNativeHistory.current = false }, 0)
    }).catch((error) => setNotice(error instanceof Error ? error.message : 'History open failed'))
  }, [selected, nativeHistoryState])

  useEffect(() => {
    const assetId = selected.libraryAsset?.id
    const before = pendingNativeBefore.current
    if (!assetId || !before || applyingNativeHistory.current) return
    if (nativeHistoryTimer.current !== null) window.clearTimeout(nativeHistoryTimer.current)
    nativeHistoryTimer.current = window.setTimeout(() => {
      pendingNativeBefore.current = null
      if (JSON.stringify(before) === JSON.stringify(nativeHistoryState)) return
      void commitNativeHistory(assetId, 'Edit interaction', 'sharedGraph', before, nativeHistoryState)
        .then(setNativeHistory).catch((error) => setNotice(error instanceof Error ? error.message : 'History commit failed'))
    }, 220)
    return () => { if (nativeHistoryTimer.current !== null) window.clearTimeout(nativeHistoryTimer.current) }
  }, [nativeHistoryState, selected.libraryAsset?.id])
  const activeLayer = selected.layers.find((layer) => layer.id === selectedLayerId)
  const activeLayerIsBrush = Boolean(activeLayer && 'type' in activeLayer.mask && activeLayer.mask.type === 'brush')
  const filteredPhotos = useMemo(() => photos.filter((photo) => {
    if (filter === 'recent') return photo.imported
    if (filter === 'five-star') return photo.rating === 5
    if (filter === 'edited') return hasPhotoEdits(photo)
    return true
  }), [filter, photos])

  const counts = useMemo(() => ({
    all: photos.length,
    recent: photos.filter((photo) => photo.imported).length,
    five: photos.filter((photo) => photo.rating === 5).length,
    edited: photos.filter(hasPhotoEdits).length,
  }), [photos])

  function chooseFilter(next: LibraryFilter) {
    setFilter(next)
    const first = photos.find((photo) => next === 'all' || (next === 'recent' && photo.imported) || (next === 'five-star' && photo.rating === 5) || (next === 'edited' && hasPhotoEdits(photo)))
    if (first) selectPhoto(first.id)
  }

  function importPhotos(files: FileList | null) {
    if (!files?.length) return
    const supported = [...files].filter((file) => file.type.startsWith('image/'))
    if (!supported.length) {
      setNotice('No browser-readable images were selected. Use JPEG, PNG or WebP.')
      return
    }
    const imported = supported.map<PhotoItem>((file) => {
      const src = URL.createObjectURL(file)
      objectUrls.current.add(src)
      return { id: crypto.randomUUID(), name: file.name, src, renderBackend: 'browserFallback', imported: true, rating: 0,
        adjustments: { ...defaultAdjustments }, curvePoints: copyCurve(defaultCurvePoints), curveChannels: defaultCurveChannels(), whiteBalanceMode: 'sourceDefault', whiteBalanceSample: null,
        opticsState: { ...defaultNativeOpticsState }, mask: { ...defaultMask }, layers: [], skinRetouch: defaultNativeSkinRetouch(), healingOperations: [], history: [], future: [] }
    })
    setPhotos((current) => [...imported, ...current])
    selectPhoto(imported[0].id)
    setFilter('all')
    setView('edit')
    setBefore(false)
    setNotice(`${imported.length} photo${imported.length === 1 ? '' : 's'} imported`)
  }

  async function refreshLibrary(queryText = librarySearch) {
    if (!nativeRuntimeAvailable()) return
    setLibraryBusy(true)
    try {
      const assets = await queryNativeLibrary({ text: queryText.trim() || null, limit: 500, sort: 'importTime', direction: 'descending' })
      const existing = new Map(photos.filter((photo) => photo.libraryAsset).map((photo) => [photo.libraryAsset!.id, photo]))
      const rows = await Promise.all(assets.map(async (asset) => {
        const current = existing.get(asset.id)
        if (current) return { ...current, name: asset.sourcePath.split(/[\\/]/).pop() ?? asset.sourcePath, sourcePath: asset.sourcePath, rating: asset.rating, libraryAsset: asset }
        const thumbnail = await nativeLibraryThumbnail(asset.id).catch(() => '')
        return libraryPhoto(asset, thumbnail)
      }))
      setLibraryAssets(assets)
      setPhotos((current) => [...rows, ...current.filter((photo) => !photo.libraryAsset)])
      setSelectedLibraryIds((current) => current.filter((id) => assets.some((asset) => asset.id === id)))
    } finally { setLibraryBusy(false) }
  }

  async function importLibraryFolder() {
    const root = await chooseNativeLibraryFolder()
    if (!root) return
    setLibraryBusy(true)
    try {
      const result = await importNativeLibraryFolder(root)
      await refreshLibrary()
      setNotice(`Library import · ${result.imported.length} added · ${result.duplicates.length} duplicate · ${result.unsupported.length} unsupported`)
    } catch (error) { setNotice(error instanceof Error ? error.message : 'Library import failed') }
    finally { setLibraryBusy(false) }
  }

  function selectLibraryAsset(event: MouseEvent, index: number, asset: NativeLibraryAsset) {
    if (event.shiftKey && libraryAnchor.current !== null) {
      const start = Math.min(libraryAnchor.current, index); const end = Math.max(libraryAnchor.current, index)
      setSelectedLibraryIds(libraryAssets.slice(start, end + 1).map((value) => value.id))
    } else if (event.ctrlKey || event.metaKey) {
      setSelectedLibraryIds((current) => current.includes(asset.id) ? current.filter((id) => id !== asset.id) : [...current, asset.id])
      libraryAnchor.current = index
    } else { setSelectedLibraryIds([asset.id]); libraryAnchor.current = index }
    setSelectedId(`library-${asset.id}`)
  }

  async function updateLibraryWorkflow(values: { rating?: number; flag?: NativeAssetFlag; colorLabel?: NativeColorLabel }) {
    if (!selectedLibraryIds.length) return
    await updateNativeLibraryWorkflow(selectedLibraryIds, values)
    await refreshLibrary()
  }

  async function addLibraryKeyword(keyword: string) {
    if (!selectedLibraryIds.length) return
    await addNativeLibraryKeywords(selectedLibraryIds, [keyword])
    await refreshLibrary()
  }

  async function requestPhotoImport() {
    if (!nativeRuntimeAvailable()) {
      fileInput.current?.click()
      return
    }
    try {
      const paths = await chooseNativePhotoPaths()
      if (!paths.length) return
      const imported = paths.map<PhotoItem>((sourcePath) => ({
        id: crypto.randomUUID(),
        name: sourcePath.split(/[\\/]/).at(-1) ?? sourcePath,
        src: nativeThumbnailUrl(sourcePath),
        sourcePath,
        renderBackend: 'native',
        imported: true,
        rating: 0,
        adjustments: { ...defaultAdjustments },
        curvePoints: copyCurve(defaultCurvePoints),
        curveChannels: defaultCurveChannels(),
        whiteBalanceMode: 'sourceDefault',
        whiteBalanceSample: null,
        opticsState: { ...defaultNativeOpticsState },
        mask: { ...defaultMask },
        layers: [],
        skinRetouch: defaultNativeSkinRetouch(),
        healingOperations: [],
        history: [],
        future: [],
      }))
      setPhotos((current) => [...imported, ...current])
      selectPhoto(imported[0].id)
      setFilter('all')
      setView('edit')
      setBefore(false)
      setNotice(`${imported.length} photo${imported.length === 1 ? '' : 's'} imported into Native preview`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Native photo picker failed')
    }
  }

  function updateSelected(mutator: (photo: PhotoItem) => PhotoItem) {
    setPhotos((current) => current.map((photo) => {
      if (photo.id !== selected.id) return photo
      const next = mutator(photo)
      if (photo.libraryAsset && next.history.length > photo.history.length && !pendingNativeBefore.current) {
        pendingNativeBefore.current = toNativeSettings(photo.adjustments, photo.curvePoints, photo.whiteBalanceMode,
          photo.whiteBalanceSample, photo.curveChannels, photo.opticsState, photo.layers, photo.mask,
          photo.skinRetouch, photo.healingOperations)
      }
      return next
    }))
  }

  function removePhoto(id: string) {
    if (photos.length <= 1) {
      setNotice('Keep at least one photo in the workspace')
      return
    }
    const removed = photos.find((photo) => photo.id === id)
    const remaining = photos.filter((photo) => photo.id !== id)
    setPhotos(remaining)
    if (selectedId === id) selectPhoto(remaining[0].id)
    if (removed?.src.startsWith('blob:')) {
      URL.revokeObjectURL(removed.src)
      objectUrls.current.delete(removed.src)
    }
    setBefore(false)
    setNotice(`${removed?.name ?? 'Photo'} removed from Starroom · source file was not deleted`)
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      if (event.key !== 'Delete' || target?.matches('input, select, textarea')) return
      removePhoto(selectedId)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  })

  function adjust(key: AdjustmentKey, value: number, recordHistory = true) {
    updateSelected((photo) => {
      const normalizedValue = key === 'cropLeft' ? Math.min(value, photo.adjustments.cropRight - 1)
        : key === 'cropTop' ? Math.min(value, photo.adjustments.cropBottom - 1)
          : key === 'cropRight' ? Math.max(value, photo.adjustments.cropLeft + 1)
            : key === 'cropBottom' ? Math.max(value, photo.adjustments.cropTop + 1) : value
      if (photo.adjustments[key] === normalizedValue) return photo
      return {
        ...photo,
        adjustments: { ...photo.adjustments, [key]: normalizedValue },
        history: recordHistory ? [...photo.history, takeSnapshot(photo)].slice(-100) : photo.history,
        future: [],
      }
    })
    setBefore(false)
  }

  function beginInteractiveEdit() {
    updateSelected((photo) => ({ ...photo, history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
  }

  function updateCurve(points: ToneCurvePoint[]) {
    updateSelected((photo) => ({ ...photo, curvePoints: curveChannel === 'master' ? copyCurve(points) : photo.curvePoints,
      curveChannels: { ...photo.curveChannels, [curveChannel]: copyCurve(points) }, future: [] }))
    setBefore(false)
  }

  const selectedNativeSettings = () => toNativeSettings(
    selected.adjustments, selected.curvePoints, selected.whiteBalanceMode,
    selected.whiteBalanceSample, selected.curveChannels, selected.opticsState,
    selected.layers, selected.mask, selected.skinRetouch, selected.healingOperations,
  )

  function applyWorkflowSettings(settings: ReturnType<typeof selectedNativeSettings>, label: string) {
    const mapped = fromNativeSettings(selected.adjustments, settings)
    updateSelected((photo) => ({ ...photo, adjustments: mapped.adjustments,
      curveChannels: mapped.curves, curvePoints: copyCurve(mapped.curves.master),
      history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
    setNotice(label)
  }

  async function selectReference() {
    const path = await chooseNativeReferencePath()
    if (!path) return
    setReferencePath(path)
    setReferenceResult(null)
    setReferenceBase(null)
    setNotice('Reference selected · Analyze to build a Native recipe')
  }

  async function analyzeReference() {
    if (!selected.sourcePath || selected.renderBackend !== 'native') return
    try {
      let path = referencePath
      if (!path) {
        path = await chooseNativeReferencePath()
        if (!path) return
        setReferencePath(path)
      }
      const base = selectedNativeSettings()
      const result = await matchNativeReference(selected.sourcePath, path, base, {
        amount: referenceControls.amount / 100,
        tone: referenceControls.tone / 100,
        color: referenceControls.color / 100,
        grading: referenceControls.grading / 100,
        protectSkin: referenceControls.protectSkin / 100,
      })
      setReferenceBase(base)
      setReferenceResult(result)
      setNotice(`Reference analyzed · ${Math.round(result.recipe.confidence * 100)}% confidence`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Reference match failed')
    }
  }

  function previewReference() {
    if (!referenceResult) return
    applyWorkflowSettings(referenceResult.settings, 'Reference preview applied through the Native graph')
  }

  function applyReference() {
    if (!referenceResult) return
    applyWorkflowSettings(referenceResult.settings, `Reference match applied · ${Math.round(referenceResult.recipe.confidence * 100)}% confidence`)
  }

  function resetReference() {
    if (referenceBase) applyWorkflowSettings(referenceBase, 'Reference preview reset')
    setReferenceResult(null)
    setReferenceBase(null)
  }

  async function saveReferenceAsLook() {
    if (!referenceResult || selected.renderBackend !== 'native') return
    try {
      const path = await chooseNativeLookPath('save', 'reference-match.srlook')
      if (!path) return
      await saveNativeLook(path, 'Reference Match', referenceResult.settings)
      setNotice('Reference recipe saved as portable .srlook')
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Reference look save failed')
    }
  }

  async function saveLookWorkflow() {
    if (selected.renderBackend !== 'native') return
    try {
      const path = await chooseNativeLookPath('save', `${selected.name.replace(/\.[^.]+$/, '')}.srlook`)
      if (!path) return
      await saveNativeLook(path, selected.name.replace(/\.[^.]+$/, ''), selectedNativeSettings())
      setNotice('Portable .srlook saved')
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Look save failed')
    }
  }

  async function loadLookWorkflow() {
    if (selected.renderBackend !== 'native') return
    try {
      const path = await chooseNativeLookPath('open')
      if (!path) return
      const settings = await applyNativeLook(path, lookAmount / 100, selectedNativeSettings())
      applyWorkflowSettings(settings, `Look applied at ${lookAmount}%`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Look load failed')
    }
  }

  async function selectMixerLook(side: 'a' | 'b') {
    const path = await chooseNativeLookPath('open')
    if (!path) return
    if (side === 'a') setLookAPath(path)
    else setLookBPath(path)
  }

  async function applyStyleMixer() {
    if (!lookAPath || !lookBPath || selected.renderBackend !== 'native') return
    try {
      const settings = await mixNativeLooks(
        lookAPath,
        lookBPath,
        lookAWeight,
        lookBWeight,
        lookAmount / 100,
        selectedNativeSettings(),
      )
      applyWorkflowSettings(settings, `Style mix applied · A ${lookAWeight} / B ${lookBWeight}`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Style mix failed')
    }
  }

  function mutateLayers(mutator: (layers: NativeAdjustmentLayer[]) => NativeAdjustmentLayer[]) {
    updateSelected((photo) => ({ ...photo, layers: mutator(copyLayers(photo.layers)), history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
  }

  function addLayer() {
    const layer = defaultLayer()
    mutateLayers((layers) => [...layers, layer])
    setSelectedLayerId(layer.id)
  }

  function duplicateLayer(id: string) {
    let duplicateId = ''
    mutateLayers((layers) => layers.flatMap((layer) => {
      if (layer.id !== id) return [layer]
      const copy = { ...layer, id: crypto.randomUUID(), name: `${layer.name} copy`, adjustments: { tone: { ...layer.adjustments.tone } } }
      duplicateId = copy.id
      return [layer, copy]
    }))
    setSelectedLayerId(duplicateId)
  }

  function deleteLayer(id: string) {
    mutateLayers((layers) => layers.filter((layer) => layer.id !== id))
    setSelectedLayerId(null)
  }

  function updateLayer(id: string, mutate: (layer: NativeAdjustmentLayer) => NativeAdjustmentLayer) {
    mutateLayers((layers) => layers.map((layer) => layer.id === id ? mutate(layer) : layer))
  }

  function moveLayer(id: string, direction: -1 | 1) {
    mutateLayers((layers) => {
      const index = layers.findIndex((layer) => layer.id === id)
      const next = index + direction
      if (index < 0 || next < 0 || next >= layers.length) return layers
      const reordered = [...layers]
      ;[reordered[index], reordered[next]] = [reordered[next], reordered[index]]
      return reordered
    })
  }

  function saveCurvePreset() {
    setSavedCurvePreset(copyCurveChannels(selected.curveChannels))
    setNotice('Custom curve preset saved')
  }

  function loadCurvePreset() {
    if (!savedCurvePreset) return
    updateSelected((photo) => ({ ...photo, curvePoints: copyCurve(savedCurvePreset.master), curveChannels: copyCurveChannels(savedCurvePreset),
      history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
    setNotice('Custom curve preset loaded')
  }

  function updateWhiteBalance(mode: NativeWhiteBalanceMode, sample: NativeWhiteBalanceSample | null = null) {
    updateSelected((photo) => ({ ...photo, whiteBalanceMode: mode, whiteBalanceSample: sample,
      history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
  }

  function copyWhiteBalance() {
    setCopiedWhiteBalance({ whiteBalanceMode: selected.whiteBalanceMode,
      whiteBalanceSample: selected.whiteBalanceSample ? { ...selected.whiteBalanceSample } : null })
    setNotice('White balance copied')
  }

  function pasteWhiteBalance() {
    if (!copiedWhiteBalance) { setNotice('Copy a white balance first'); return }
    updateWhiteBalance(copiedWhiteBalance.whiteBalanceMode,
      copiedWhiteBalance.whiteBalanceSample ? { ...copiedWhiteBalance.whiteBalanceSample } : null)
    setNotice('White balance pasted')
  }

  async function pickMixerBand(x: number, y: number) {
    if (!selected.sourcePath || selected.renderBackend !== 'native') {
      setNotice('Color Mixer targeting requires a Native photo; no Browser color fallback was used.')
      return
    }
    try {
      const band = await sampleNativeColor(selected.sourcePath, x, y, selected.adjustments, selected.curvePoints,
        selected.whiteBalanceMode, selected.whiteBalanceSample, selected.curveChannels, selected.opticsState)
      if (!band) { setNotice('The sampled area is neutral; no color band was selected.'); return }
      setMixerBand(`${band[0].toUpperCase()}${band.slice(1)}`)
      setMixerPicking(false)
      setNotice(`${band} band selected from Native working color`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Native Color Mixer sampling failed')
    }
  }

  function updateMask(mask: RadialMask) {
    updateSelected((photo) => ({ ...photo, mask: { ...mask }, future: [] }))
    setBefore(false)
  }

  function updateOpticsState(opticsState: NativeOpticsState) {
    updateSelected((photo) => ({ ...photo, opticsState, history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setOpticsStatus(null)
    setBefore(false)
  }

  async function refreshOpticsStatus() {
    if (!selected.sourcePath || selected.renderBackend !== 'native') {
      setNotice('Lensfun profile resolution requires a Native photo; Browser fallback was not used.')
      return
    }
    try {
      const status = await resolveNativeOpticsStatus(selected.sourcePath, selected.adjustments, selected.curvePoints,
        selected.whiteBalanceMode, selected.whiteBalanceSample, selected.curveChannels, selected.opticsState)
      setOpticsStatus(status)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Lensfun resolution failed')
    }
  }

  async function detectPortrait() {
    if (!selected.sourcePath || selected.renderBackend !== 'native') {
      setNotice('Portrait detection requires a Native photo; Browser fallback is not used.')
      return
    }
    setRenderStatus('Local YuNet + BiSeNet detection…')
    try {
      const detection = await detectNativePortrait(selected.sourcePath)
      setPortraitDetection(detection)
      setPortraitFaceId(detection.faces[0]?.face.id ?? null)
      const message = detection.status === 'ready' ? `${detection.faces.length} local face(s) detected` : detection.error?.message ?? 'No face detected'
      setNotice(message)
      setRenderStatus(detection.status === 'ready' ? 'Portrait masks ready in Native cache' : `Portrait ${detection.status}`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Portrait detection failed')
      setRenderStatus('Portrait detection failed')
    }
  }

  function addPortraitMask(faceId: string, cacheKey: string, region: NativePortraitRegion) {
    if (!portraitDetection) return
    const layer = defaultLayer()
    const label = region.replace(/([A-Z])/g, ' $1')
    layer.name = `Portrait ${label}`
    layer.mask = { type: 'portraitSemantic', faceId, region, threshold: .5, feather: .08,
      modelId: portraitDetection.parserModelId, modelVersion: portraitDetection.parserModelVersion,
      modelHash: portraitDetection.parserModelHash, cacheKey }
    mutateLayers((layers) => [...layers, layer])
    setSelectedLayerId(layer.id)
    setNotice(`${layer.name} added as a Native MaskTree leaf`)
  }

  function addAllPortraitMasks(region: NativePortraitRegion) {
    if (!portraitDetection?.faces.length) return
    const layer = defaultLayer()
    const label = region.replace(/([A-Z])/g, ' $1')
    layer.name = `All faces ${label}`
    // This is metadata-only UI composition.  Rust evaluates the M15 MaskTree
    // and all semantic probability rasters in the native shared render graph.
    layer.mask = {
      operation: 'add',
      children: portraitDetection.faces.map(({ face, cacheKey }) => ({
        type: 'portraitSemantic' as const,
        faceId: face.id,
        region,
        threshold: .5,
        feather: .08,
        modelId: portraitDetection.parserModelId,
        modelVersion: portraitDetection.parserModelVersion,
        modelHash: portraitDetection.parserModelHash,
        cacheKey,
      })),
    }
    mutateLayers((layers) => [...layers, layer])
    setSelectedLayerId(layer.id)
    setNotice(`${layer.name} added as a Native MaskTree group`)
  }

  async function generateAiMask(semantic: Extract<NativeAiMaskSemantic, 'subject' | 'background' | 'sky'>) {
    if (!selected.sourcePath || selected.renderBackend !== 'native') {
      setNotice('AI Mask requires a Native photo; Browser fallback is intentionally unavailable.')
      return
    }
    const requestId = crypto.randomUUID()
    setAiMaskRequestId(requestId)
    setRenderStatus(`Generating ${semantic} mask locally…`)
    try {
      const result = await generateNativeAiMask(selected.sourcePath, semantic, requestId)
      const layer = defaultLayer()
      layer.name = `AI ${semantic}`
      layer.mask = {
        type: 'generated', providerId: result.providerId, modelId: result.modelId,
        modelVersion: result.modelVersion, modelHash: result.modelHash,
        semanticClass: result.semanticClass, threshold: .5, feather: .08, invert: false,
        cacheIdentity: result.cacheIdentity,
        metadata: { executionProvider: result.executionProvider, source: 'local-only', status: result.status },
      }
      mutateLayers((layers) => [...layers, layer])
      setSelectedLayerId(layer.id)
      setAiMaskResult(result)
      setMaskOverlayVisible(true)
      setNotice(`${semantic} mask ${result.status} · ${result.executionProvider === 'directMl' ? 'DirectML' : 'CPU fallback'}`)
      setRenderStatus(`AI Mask · ${result.executionProvider === 'directMl' ? 'DirectML' : 'CPU fallback'}`)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : `${semantic} mask generation failed`)
      setRenderStatus('AI Mask unavailable')
    } finally {
      setAiMaskRequestId(null)
    }
  }

  async function cancelAiMask() {
    if (!aiMaskRequestId) return
    await cancelNativeAiMask(aiMaskRequestId)
    setNotice('AI mask cancellation requested')
  }

  function updateSkinRetouch(mutator: (current: NativeSkinRetouchSettings) => NativeSkinRetouchSettings) {
    updateSelected((photo) => ({ ...photo, skinRetouch: copySkinRetouch(mutator(copySkinRetouch(photo.skinRetouch))), history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
  }

  function enableSkinRetouch(faceId: string | '__all__') {
    if (!portraitDetection?.faces.length) {
      setNotice('Detect a portrait locally before enabling Skin retouch')
      return
    }
    const faces = portraitDetection.faces
      .filter(({ face }) => faceId === '__all__' || face.id === faceId)
      .map(({ face, cacheKey }) => ({ faceId: face.id, cacheKey }))
    updateSkinRetouch((current) => ({ ...current, faces }))
    setNotice(`Skin retouch linked to ${faces.length} local portrait cache entr${faces.length === 1 ? 'y' : 'ies'}`)
  }

  function updateHealingOperations(mutator: (current: NativeHealingOperation[]) => NativeHealingOperation[]) {
    updateSelected((photo) => ({ ...photo, healingOperations: copyHealingOperations(mutator(copyHealingOperations(photo.healingOperations))).slice(0, 256), history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
  }

  async function runAdvisor() {
    if (selected.renderBackend !== 'native' || !selected.sourcePath) {
      setNotice('Advisor requires a Native image; Browser fallback is intentionally unavailable.')
      return
    }
    try {
      const result = await adviseNativeImage(selected.sourcePath, selected.adjustments, selected.curvePoints, selected.whiteBalanceMode, selected.whiteBalanceSample,
        selected.curveChannels, selected.opticsState, selected.layers, selected.skinRetouch, selected.healingOperations)
      setAdvisorResult(result)
      setNotice(`${result.suggestions.length} local, explainable suggestion${result.suggestions.length === 1 ? '' : 's'} ready`)
    } catch (error) { setNotice(error instanceof Error ? error.message : 'Native advisor failed') }
  }

  function applyAdvisorSuggestions(suggestions: NativeAdvisorSuggestion[]) {
    const allowed = new Set<AdjustmentKey>(['exposure', 'shadows', 'highlights', 'contrast', 'temperature', 'tint'])
    updateSelected((photo) => {
      const adjustments = { ...photo.adjustments }
      for (const suggestion of suggestions) {
        if (!allowed.has(suggestion.control as AdjustmentKey)) continue
        const key = suggestion.control as AdjustmentKey
        const min = key === 'exposure' ? -5 : -100
        const max = key === 'exposure' ? 5 : 100
        adjustments[key] = Math.max(min, Math.min(max, adjustments[key] + suggestion.amount))
      }
      return { ...photo, adjustments, history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }
    })
    setBefore(false)
  }

  function previewAdvisorSuggestion(suggestion: NativeAdvisorSuggestion) {
    if (!advisorPreview) setAdvisorPreview(takeSnapshot(selected))
    const allowed = new Set<AdjustmentKey>(['exposure', 'shadows', 'highlights', 'contrast', 'temperature', 'tint'])
    if (!allowed.has(suggestion.control as AdjustmentKey)) return
    const key = suggestion.control as AdjustmentKey
    const min = key === 'exposure' ? -5 : -100
    const max = key === 'exposure' ? 5 : 100
    updateSelected((photo) => ({ ...photo, adjustments: { ...photo.adjustments, [key]: Math.max(min, Math.min(max, photo.adjustments[key] + suggestion.amount)) } }))
    setBefore(false)
  }

  function cancelAdvisorPreview() {
    if (!advisorPreview) return
    updateSelected((photo) => applySnapshot(photo, advisorPreview))
    setAdvisorPreview(null)
  }

  function acceptAdvisorPreview() {
    if (!advisorPreview) return
    updateSelected((photo) => ({ ...photo, history: [...photo.history, advisorPreview].slice(-100), future: [] }))
    setAdvisorPreview(null)
  }

  function addHealingStroke(points: Array<{ x: number; y: number }>) {
    const expanded = points.flatMap((point, index) => {
      const previous = points[index - 1]
      if (!previous) return [point]
      const steps = Math.max(1, Math.ceil(Math.hypot(point.x - previous.x, point.y - previous.y) / .015))
      return Array.from({ length: steps }, (_, step) => ({ x: previous.x + (point.x - previous.x) * (step + 1) / steps, y: previous.y + (point.y - previous.y) * (step + 1) / steps }))
    })
    updateHealingOperations((current) => [...current, ...expanded.map((target) => ({
      id: crypto.randomUUID(), enabled: true, mode: 'heal' as const, target, source: null, radius: 24, feather: .55, opacity: .85,
      rotationDegrees: 0, scale: 1, toneAdaptation: true, textureAdaptation: true, sourceMode: 'auto' as const,
      metadata: { interaction: 'M18 brush', coordinateSpace: 'source-normalized' },
    }))])
    setNotice(`Added ${expanded.length} Native heal operation${expanded.length === 1 ? '' : 's'}`)
  }

  function addMaskBrushStroke(points: Array<{ x: number; y: number }>) {
    if (!selectedLayerId) return
    updateLayer(selectedLayerId, (layer) => {
      if (!('type' in layer.mask) || layer.mask.type !== 'brush') return layer
      const spacing = Math.max(.002, layer.mask.radius * .18)
      const interpolated = points.flatMap((point, index) => {
        const previous = points[index - 1]
        if (!previous) return [{ ...point, pressure: 1 }]
        const steps = Math.max(1, Math.ceil(Math.hypot(point.x - previous.x, point.y - previous.y) / spacing))
        return Array.from({ length: steps }, (_, step) => ({
          x: previous.x + (point.x - previous.x) * (step + 1) / steps,
          y: previous.y + (point.y - previous.y) * (step + 1) / steps,
          pressure: 1,
        }))
      })
      return { ...layer, mask: { ...layer.mask, points: [...layer.mask.points, ...interpolated].slice(-8192) } }
    })
    setNotice('Freehand mask stroke added in source-normalized image space')
  }

  function resetAdjustment(key: AdjustmentKey) {
    adjust(key, defaultAdjustments[key])
  }

  function applyHistoryResult(result: NativeHistoryResult) {
    applyingNativeHistory.current = true
    pendingNativeBefore.current = null
    setNativeHistory(result)
    updateSelected((photo) => applyNativeHistoryState(photo, result.state))
    window.setTimeout(() => { applyingNativeHistory.current = false }, 0)
  }

  function undo() {
    if (selected.libraryAsset) {
      void undoNativeHistory(selected.libraryAsset.id).then(applyHistoryResult).catch((error) => setNotice(error instanceof Error ? error.message : 'Undo failed'))
      return
    }
    updateSelected((photo) => {
      const previous = photo.history.at(-1)
      if (!previous) return photo
      return { ...applySnapshot(photo, previous), history: photo.history.slice(0, -1), future: [takeSnapshot(photo), ...photo.future] }
    })
  }

  function redo() {
    if (selected.libraryAsset) {
      void redoNativeHistory(selected.libraryAsset.id).then(applyHistoryResult).catch((error) => setNotice(error instanceof Error ? error.message : 'Redo failed'))
      return
    }
    updateSelected((photo) => {
      const next = photo.future[0]
      if (!next) return photo
      return { ...applySnapshot(photo, next), history: [...photo.history, takeSnapshot(photo)], future: photo.future.slice(1) }
    })
  }

  function createSnapshot() {
    if (!selected.libraryAsset) return
    void createNativeSnapshot(selected.libraryAsset.id, snapshotName).then((result) => { setNativeHistory(result); setSnapshotName(`Version ${result.snapshots.length + 1}`) })
      .catch((error) => setNotice(error instanceof Error ? error.message : 'Snapshot creation failed'))
  }

  function restoreSnapshot(snapshotId: string) {
    if (!selected.libraryAsset) return
    void restoreNativeSnapshot(selected.libraryAsset.id, snapshotId).then(applyHistoryResult)
      .catch((error) => setNotice(error instanceof Error ? error.message : 'Snapshot restore failed'))
  }

  function toggleRating() {
    updateSelected((photo) => ({ ...photo, rating: photo.rating === 5 ? 0 : 5 }))
  }

  function resetAll() {
    if (!hasPhotoEdits(selected)) return
    updateSelected((photo) => ({ ...photo, adjustments: { ...defaultAdjustments }, curvePoints: copyCurve(defaultCurvePoints), curveChannels: defaultCurveChannels(), whiteBalanceMode: 'sourceDefault', whiteBalanceSample: null,
      opticsState: { ...defaultNativeOpticsState }, mask: { ...defaultMask },
      layers: [], skinRetouch: defaultNativeSkinRetouch(), healingOperations: [],
      history: [...photo.history, takeSnapshot(photo)], future: [] }))
  }

  async function exportJpeg() {
    setRenderStatus(selected.renderBackend === 'native' ? 'Native full-resolution export…' : 'Browser fallback export…')
    try {
      if (selected.renderBackend === 'native') {
        if (!selected.sourcePath) throw new Error('Native photo is missing its source path.')
        const outputPath = await chooseNativeExportPath(selected.name)
        if (!outputPath) {
          setRenderStatus('Export cancelled')
          return
        }
        const result = await exportNativeJpeg(selected.sourcePath, outputPath, selected.adjustments, selected.curvePoints, selected.mask,
          selected.whiteBalanceMode, selected.whiteBalanceSample, selected.curveChannels, selected.opticsState, selected.layers, selected.skinRetouch, selected.healingOperations)
        setNotice(`Native JPEG exported · ${result.width} × ${result.height} · ${result.inputProfile}`)
        setRenderStatus(`Native CPU · ${result.workingSpace}`)
        return
      }
      const canvas = await renderImageSource(selected.src, selected.adjustments, Number.POSITIVE_INFINITY, selected.curvePoints, selected.mask)
      const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob((value) => value ? resolve(value) : reject(new Error('JPEG encoding failed.')), 'image/jpeg', .94))
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      const base = selected.name.replace(/\.[^.]+$/, '')
      anchor.href = url
      anchor.download = `${base}-starroom.jpg`
      anchor.click()
      window.setTimeout(() => URL.revokeObjectURL(url), 1000)
      setNotice('Browser fallback JPEG exported without overwriting the original')
      setRenderStatus('Browser fallback preview')
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Export failed')
      setRenderStatus('Export failed')
    }
  }

  return <main className={`app theme-${theme}`} data-theme={theme}>
    <AppHeader view={view} setView={(next) => { setView(next); setBefore(false) }} theme={theme} setTheme={setTheme} before={before} setBefore={setBefore}
      canUndo={selected.libraryAsset ? Boolean(nativeHistory?.canUndo) : selected.history.length > 0}
      canRedo={selected.libraryAsset ? Boolean(nativeHistory?.canRedo) : selected.future.length > 0} undo={undo} redo={redo} onExport={exportJpeg} />
    <div className={`workspace view-${view} ${leftOpen ? '' : 'left-collapsed'} ${filmstripOpen ? '' : 'filmstrip-collapsed'}`}>
      <aside className="library-panel">
        <div className="panel-title"><span>Library</span><IconButton label="Collapse library" onClick={() => setLeftOpen(false)}><PanelLeftClose size={17} /></IconButton></div>
        <button className="import-button" onClick={() => view === 'library' ? void importLibraryFolder() : void requestPhotoImport()}><ImagePlus size={16} /> {view === 'library' ? 'Import folder' : 'Add photos'}</button>
        <input ref={fileInput} type="file" accept="image/jpeg,image/png,image/webp,image/svg+xml" multiple hidden onChange={(event) => { importPhotos(event.target.files); event.target.value = '' }} />
        <span className="format-note">Native: JPEG · PNG · TIFF · NEF · ARW · CR2/CR3 · DNG · RAF</span>
        <div className="library-group"><span className="eyebrow">Workspace</span>
          <button className={`library-item ${filter === 'all' ? 'selected' : ''}`} onClick={() => chooseFilter('all')}><Grid2X2 size={16} /> All Photos <small>{counts.all}</small></button>
          <button className={`library-item ${filter === 'recent' ? 'selected' : ''}`} onClick={() => chooseFilter('recent')}><Folder size={16} /> Recent Imports <small>{counts.recent}</small></button>
        </div>
        <div className="library-group"><span className="eyebrow">Smart albums</span>
          <button className={`library-item ${filter === 'five-star' ? 'selected' : ''}`} onClick={() => chooseFilter('five-star')}><Star size={16} /> Five Stars <small>{counts.five}</small></button>
          <button className={`library-item ${filter === 'edited' ? 'selected' : ''}`} onClick={() => chooseFilter('edited')}><Contrast size={16} /> Edited <small>{counts.edited}</small></button>
        </div>
        <div className="library-summary"><Library size={15} /><span>{filteredPhotos.length} visible photos</span></div>
      </aside>
      {!leftOpen && <button className="edge-toggle left" aria-label="Open library" onClick={() => setLeftOpen(true)}><PanelLeftOpen size={17} /></button>}

      {view === 'library' ? <section className="library-browser" aria-label="Photo library">
        <div className="library-browser-head"><div><span className="eyebrow">Local-first Library</span><h1>{libraryAssets.length} assets</h1></div>
          <div className="library-actions"><input aria-label="Search Library" value={librarySearch} placeholder="Filename, camera, lens, keyword" onChange={(event) => setLibrarySearch(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') void refreshLibrary() }} />
            <button disabled={libraryBusy} onClick={() => void refreshLibrary()}>{libraryBusy ? 'Working…' : 'Search'}</button>
            <button className="import-button compact" disabled={libraryBusy} onClick={() => void importLibraryFolder()}><ImagePlus size={16} /> Import folder</button></div></div>
        <div className="photo-grid virtual-grid" role="grid" aria-rowcount={libraryAssets.length}>{libraryAssets.map((asset, index) => {
          const photo = photos.find((value) => value.libraryAsset?.id === asset.id)
          if (!photo) return null
          const selectedAsset = selectedLibraryIds.includes(asset.id)
          return <article key={asset.id} role="gridcell" className={selectedAsset ? 'photo-card selected' : 'photo-card'}>
            <button className="photo-card-preview" onClick={(event) => selectLibraryAsset(event, index, asset)} onDoubleClick={() => { selectPhoto(photo.id); setView('edit'); setBefore(false) }} title={`Select ${photo.name}; double-click to edit`}>
              {photo.src ? <img loading="lazy" src={photo.src} alt={photo.name} /> : <span className="missing-thumbnail">Thumbnail unavailable</span>}
            </button>
            <div><span title={photo.name}>{photo.name}</span><small>{asset.missing ? 'Missing' : `${asset.metadata.fileType.toUpperCase()} · ${asset.rating}★`} · {asset.keywords.join(', ') || 'No keywords'}</small></div>
          </article>
        })}</div>
      </section> : <section className="canvas-area">
          <div className="canvas-toolbar"><span>{selected.name}</span><div>
            <button className={selected.rating === 5 ? 'active rating-button' : 'rating-button'} onClick={toggleRating} title="Toggle five-star rating"><Star size={12} fill={selected.rating === 5 ? 'currentColor' : 'none'} /> {selected.rating === 5 ? '5★' : 'Rate'}</button>
            <button className="remove-selected" disabled={photos.length <= 1} onClick={() => removePhoto(selected.id)} title="Remove from Starroom; does not delete source"><Trash2 size={12} /> Remove</button>
            <button className={zoom === 'fit' && zoomScale === 1 ? 'active' : ''} onClick={() => { setZoom('fit'); setZoomScale(1); setPan({ x: 0, y: 0 }) }}>Fit</button>
            <button className={zoom === '100' ? 'active' : ''} onClick={() => { setZoom('100'); setZoomScale(1); setPan({ x: 0, y: 0 }) }}>100%</button>
          </div></div>
          {view === 'compare' ? <div className="compare-stage">
            <div className="compare-pane"><PreviewCanvas photo={selected} before zoom={zoom} metric={false} onHistogram={setHistogram} onStatus={setRenderStatus} onDimensions={setDimensions} /><span>Original</span></div>
            <div className="compare-pane"><PreviewCanvas photo={selected} before={false} zoom={zoom} onHistogram={setHistogram} onStatus={setRenderStatus} onDimensions={setDimensions} /><span>Edited</span></div>
          </div> : <div className={`photo-stage ${before ? 'show-before' : ''} zoom-stage-${zoom} ${zoomScale > 1 ? 'is-zoomed' : ''} ${(tool === 'masks' || tool === 'heal') ? 'mask-mode' : ''}`}
            onWheel={(event) => {
              const next = Math.max(.25, Math.min(6, zoomScale * Math.exp(-event.deltaY * .0015)))
              setZoom('fit')
              setZoomScale(next)
              if (next <= 1) setPan({ x: 0, y: 0 })
            }}
            onPointerDown={(event) => {
              if (tool === 'masks' || tool === 'heal' || zoomScale <= 1 || event.button !== 0) return
              panStart.current = { x: event.clientX, y: event.clientY, panX: pan.x, panY: pan.y }
              event.currentTarget.setPointerCapture(event.pointerId)
            }}
            onPointerMove={(event) => {
              if (!panStart.current) return
              setPan({ x: panStart.current.panX + event.clientX - panStart.current.x, y: panStart.current.panY + event.clientY - panStart.current.y })
            }}
            onPointerUp={(event) => { panStart.current = null; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}>
            <div className="photo-frame" style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoomScale})` }}>
              <PreviewCanvas photo={selected} before={before} zoom={zoom} maskActive={tool === 'masks' && !before && !activeLayerIsBrush}
                onBeginMaskEdit={beginInteractiveEdit} onMaskChange={updateMask}
                healActive={tool === 'heal' && !before && selected.renderBackend === 'native'} onHealingStroke={addHealingStroke}
                brushActive={tool === 'masks' && !before && activeLayerIsBrush} onBrushStroke={addMaskBrushStroke}
                maskPreview={!before && maskOverlayVisible && activeLayer && 'type' in activeLayer.mask && activeLayer.mask.type === 'generated' ? activeLayer.mask : null}
                onWhiteBalancePick={(sample) => updateWhiteBalance('neutralPicker', sample)}
                onColorSample={tool === 'color' && mixerPicking ? pickMixerBand : undefined}
                onHistogram={setHistogram} onStatus={setRenderStatus} onDimensions={setDimensions} />
              {tool === 'geometry' && !before && <div className="geometry-overlay" aria-label="Crop and geometry guides"
                style={{ left: `${selected.adjustments.cropLeft}%`, top: `${selected.adjustments.cropTop}%`,
                  width: `${selected.adjustments.cropRight - selected.adjustments.cropLeft}%`,
                  height: `${selected.adjustments.cropBottom - selected.adjustments.cropTop}%` }}>
                <i className="guide-v one" /><i className="guide-v two" /><i className="guide-h one" /><i className="guide-h two" />
              </div>}
              {tool === 'geometry' && !before && selected.adjustments.geometryFourPoint !== 0
                && <FourPointOverlay values={selected.adjustments} onBeginEdit={beginInteractiveEdit} onAdjust={adjust} />}
              {tool === 'masks' && !before && portraitDetection?.status === 'ready' && <div className="portrait-overlay" aria-label="Detected portrait regions">
                {portraitDetection.faces.map(({ face }, index) => <button key={face.id} className={portraitFaceId === face.id || portraitFaceId === '__all__' ? 'selected' : ''}
                  style={{ left: `${face.bounds.left * 100}%`, top: `${face.bounds.top * 100}%`, width: `${(face.bounds.right - face.bounds.left) * 100}%`, height: `${(face.bounds.bottom - face.bounds.top) * 100}%` }}
                  onClick={() => setPortraitFaceId(face.id)}>Face {index + 1}</button>)}
              </div>}
              <span className="preview-badge">{selected.renderBackend === 'native' ? 'Native CPU' : 'Browser fallback'} · {before ? 'Original' : hasPhotoEdits(selected) ? `${countPhotoEdits(selected)} edits` : 'Original'}</span>
            </div>
          </div>}
          <div className="canvas-footer"><span>{zoomScale !== 1 ? `${Math.round(zoomScale * 100)}%` : zoom === 'fit' ? 'Fit' : '100%'}</span><span className="status-dot" /><span>{renderStatus}</span><span>· {dimensions}</span>
            <span className="zoom-help"><Move size={12} /> Wheel to zoom · drag to pan</span>
            <button aria-label="Toggle filmstrip" onClick={() => setFilmstripOpen(!filmstripOpen)}>{filmstripOpen ? <PanelBottomClose size={16} /> : <PanelBottomOpen size={16} />}</button></div>
          <div className="filmstrip" aria-label="Filmstrip">
            {filteredPhotos.length ? filteredPhotos.map((photo) => <div key={photo.id} className="thumb-shell">
              <button className={photo.id === selected.id ? 'thumb active' : 'thumb'} onClick={() => { selectPhoto(photo.id); setBefore(false) }} title={photo.name}>
                <img src={photo.src} alt={photo.name} /><span>{photo.rating === 5 ? '★' : hasPhotoEdits(photo) ? 'E' : ''}</span>
              </button>
              <button className="thumb-delete" aria-label={`Remove ${photo.name}`} title="Remove from Starroom (source stays on disk)" disabled={photos.length <= 1} onClick={() => removePhoto(photo.id)}><Trash2 size={13} /></button>
            </div>) : <div className="empty-filmstrip">No photos match this album.</div>}
          </div>
        </section>}

      <aside className="inspector-panel">
        {view === 'library' && <LibraryMetadataPanel asset={libraryAssets.find((asset) => asset.id === selectedLibraryIds.at(-1)) ?? null}
          selectedCount={selectedLibraryIds.length} onWorkflow={(values) => void updateLibraryWorkflow(values)} onAddKeyword={(keyword) => void addLibraryKeyword(keyword)} />}
        <div className="histogram-wrap"><Histogram values={histogram} /><div><span>LIVE</span><span>{dimensions}</span><span>CPU</span></div></div>
        <section className="portrait-panel" aria-label="Portrait masks">
          <div className="layer-stack-head"><strong>Portrait</strong><button onClick={detectPortrait} disabled={selected.renderBackend !== 'native'}>Detect faces</button></div>
          {selected.renderBackend !== 'native' && <small>Native image required. Browser fallback is intentionally unavailable.</small>}
          {portraitDetection && <div className={`portrait-status status-${portraitDetection.status}`}>
            <strong>{portraitDetection.status === 'ready' ? `${portraitDetection.faces.length} face(s)` : portraitDetection.status}</strong>
            <small>{portraitDetection.error?.message ?? `YuNet ${portraitDetection.detectorModelVersion.slice(0, 8)} · BiSeNet ResNet18 · ${portraitDetection.executionProvider === 'directMl' ? 'DirectML' : 'CPU'}`}</small>
          </div>}
          {portraitDetection?.faces.map(({ face, cacheKey }, index) => <div className={portraitFaceId === face.id ? 'portrait-face selected' : 'portrait-face'} key={face.id}>
            <button onClick={() => setPortraitFaceId(face.id)}>Face {index + 1} · {Math.round(face.confidence * 100)}%</button>
            {portraitFaceId === face.id && <div className="portrait-regions">{(['face', 'skin', 'eyes', 'brows', 'lips', 'hair'] as NativePortraitRegion[]).map((region) =>
              <button key={region} onClick={() => addPortraitMask(face.id, cacheKey, region)}>{region}</button>)}</div>}
          </div>)}
          {portraitDetection?.faces.length && <div className={portraitFaceId === '__all__' ? 'portrait-face selected' : 'portrait-face'}>
            <button onClick={() => setPortraitFaceId('__all__')}>All faces</button>
            {portraitFaceId === '__all__' && <div className="portrait-regions">{(['face', 'skin', 'eyes', 'brows', 'lips', 'hair'] as NativePortraitRegion[]).map((region) =>
              <button key={region} onClick={() => addAllPortraitMasks(region)}>{region}</button>)}</div>}
          </div>}
          <div className="skin-retouch-panel" aria-label="AI Mask">
            <div className="layer-stack-head"><strong>AI Mask</strong>{aiMaskRequestId && <button onClick={cancelAiMask}>Cancel</button>}</div>
            <small>Local ONNX only · editable M15 MaskTree leaf · no pixels cross IPC</small>
            <div className="portrait-regions">
              {(['subject', 'background', 'sky'] as const).map((semantic) => <button key={semantic} disabled={selected.renderBackend !== 'native' || aiMaskRequestId !== null} onClick={() => generateAiMask(semantic)}>{semantic}</button>)}
              <button disabled={!portraitDetection?.faces.length} onClick={() => addAllPortraitMasks('face')}>person</button>
              <button disabled={!portraitDetection?.faces.length} onClick={() => addAllPortraitMasks('skin')}>skin</button>
              <button disabled={!portraitDetection?.faces.length} onClick={() => addAllPortraitMasks('hair')}>hair</button>
            </div>
            {aiMaskRequestId && <small>Generating locally… cancellation remains available.</small>}
            {aiMaskResult && <small>{aiMaskResult.semanticClass} · {aiMaskResult.executionProvider === 'directMl' ? 'DirectML' : 'CPU fallback'} · {aiMaskResult.status}</small>}
            <label><input type="checkbox" checked={maskOverlayVisible} disabled={!activeLayer || !('type' in activeLayer.mask) || activeLayer.mask.type !== 'generated'} onChange={(event) => setMaskOverlayVisible(event.target.checked)} /> Mask overlay</label>
          </div>
          <div className="skin-retouch-panel" aria-label="Skin retouch">
            <div className="layer-stack-head"><strong>Skin retouch</strong><button onClick={() => enableSkinRetouch(portraitFaceId === '__all__' ? '__all__' : portraitFaceId ?? '__all__')} disabled={!portraitDetection?.faces.length}>Use selected face</button></div>
            {selected.skinRetouch.faces.length === 0
              ? <small>Choose a locally detected face. Skin is automatically protected from eyes, brows, lips and hair.</small>
              : <small>{selected.skinRetouch.faces.length} cached face{selected.skinRetouch.faces.length === 1 ? '' : 's'} · Native shared graph</small>}
            <div className="mask-controls">
              {([
                ['Smooth', 'smooth', 0, 100, 1], ['Texture preserve', 'texture', 0, 100, 1], ['Tone evenness', 'toneEvenness', 0, 100, 1],
                ['Skin hue', 'hueDegrees', -30, 30, 1], ['Skin chroma', 'chroma', -50, 50, 1], ['Face exposure', 'exposureEv', -2, 2, .05],
              ] as const).map(([label, key, min, max, step]) => {
                const raw = selected.skinRetouch.parameters[key]
                const value = key === 'texture' || key === 'smooth' || key === 'toneEvenness' ? Math.round(raw * 100) : key === 'chroma' ? Math.round(raw * 100) : raw
                return <label key={key}>{label}<input aria-label={`Skin ${label}`} type="number" min={min} max={max} step={step} value={value}
                  disabled={selected.skinRetouch.faces.length === 0}
                  onChange={(event) => { const next = Number(event.target.value); if (!Number.isFinite(next)) return; updateSkinRetouch((current) => ({ ...current, parameters: { ...current.parameters, [key]: key === 'texture' || key === 'smooth' || key === 'toneEvenness' || key === 'chroma' ? next / 100 : next } })) }} /></label>
              })}
            </div>
          </div>
          <div className="skin-retouch-panel" aria-label="Healing brush">
            <div className="layer-stack-head"><strong>Healing brush</strong><button onClick={() => setTool('heal')} disabled={selected.renderBackend !== 'native'}>Brush</button></div>
            <small>Drag on the Native preview to create zoom-independent, feathered heal strokes. Auto Source is deterministic; AI inpaint is intentionally unavailable.</small>
            {selected.healingOperations.length > 0 && (() => {
              const operation = selected.healingOperations.at(-1)!
              const patch = (changes: Partial<NativeHealingOperation>) => updateHealingOperations((current) => current.map((value, index) => index === current.length - 1 ? { ...value, ...changes } : value))
              return <div className="mask-controls">
                <label>Mode<select aria-label="Healing mode" value={operation.mode} onChange={(event) => patch({ mode: event.target.value as NativeHealingOperation['mode'] })}><option value="heal">Heal</option><option value="clone">Clone</option></select></label>
                <label>Source<select aria-label="Healing source mode" value={operation.sourceMode} onChange={(event) => patch({ sourceMode: event.target.value as NativeHealingOperation['sourceMode'], source: event.target.value === 'manual' ? operation.source ?? { x: .5, y: .5 } : null })}><option value="auto">Auto</option><option value="manual">Manual</option></select></label>
                {([['Radius', 'radius', .5, 512, .5], ['Feather', 'feather', 0, 1, .01], ['Opacity', 'opacity', 0, 1, .01], ['Angle', 'rotationDegrees', -180, 180, 1], ['Scale', 'scale', .1, 4, .01]] as const).map(([label, key, min, max, step]) => <label key={key}>{label}<input aria-label={`Healing ${label}`} type="number" value={operation[key]} min={min} max={max} step={step} onChange={(event) => { const value = Number(event.target.value); if (Number.isFinite(value)) patch({ [key]: Math.max(min, Math.min(max, value)) }) }} /></label>)}
                {operation.sourceMode === 'manual' && <><label>Source X<input aria-label="Healing source X" type="number" min="0" max="1" step=".01" value={operation.source?.x ?? .5} onChange={(event) => patch({ source: { x: Math.max(0, Math.min(1, Number(event.target.value) || 0)), y: operation.source?.y ?? .5 } })} /></label><label>Source Y<input aria-label="Healing source Y" type="number" min="0" max="1" step=".01" value={operation.source?.y ?? .5} onChange={(event) => patch({ source: { x: operation.source?.x ?? .5, y: Math.max(0, Math.min(1, Number(event.target.value) || 0)) } })} /></label></>}
                <label><input type="checkbox" checked={operation.toneAdaptation} onChange={(event) => patch({ toneAdaptation: event.target.checked })} /> Tone adapt</label><label><input type="checkbox" checked={operation.textureAdaptation} onChange={(event) => patch({ textureAdaptation: event.target.checked })} /> Texture adapt</label>
                <button onClick={() => updateHealingOperations((current) => current.slice(0, -1))}>Remove last</button><small>{selected.healingOperations.length} operation{selected.healingOperations.length === 1 ? '' : 's'}</small>
              </div>
            })()}
          </div>
          <div className="skin-retouch-panel" aria-label="Local edit advisor">
            <div className="layer-stack-head"><strong>Local advisor</strong><button onClick={runAdvisor} disabled={selected.renderBackend !== 'native'}>Analyze</button></div>
            <small>Deterministic local statistics and explicit rules. No cloud, GPT, or ML confidence score.</small>
            {advisorResult && <div className="advisor-results"><small>p01 {advisorResult.analysis.p01.toFixed(3)} · p50 {advisorResult.analysis.p50.toFixed(3)} · p99 {advisorResult.analysis.p99.toFixed(3)}</small>
              {advisorPreview && <div className="portrait-regions"><button onClick={acceptAdvisorPreview}>Apply preview</button><button onClick={cancelAdvisorPreview}>Cancel preview</button></div>}
              <button disabled={!advisorResult.suggestions.length} onClick={() => { applyAdvisorSuggestions(advisorResult.suggestions); setAdvisorResult(null) }}>Apply all safe</button><button onClick={() => setAdvisorResult(null)}>Dismiss</button>
              {advisorResult.suggestions.map((suggestion) => <div key={suggestion.id} className="portrait-face"><strong>{suggestion.what}</strong><small>{suggestion.why} · {suggestion.confidence}</small><span>{suggestion.control} {suggestion.amount > 0 ? '+' : ''}{suggestion.amount.toFixed(suggestion.control === 'exposure' ? 2 : 0)}</span><button onClick={() => previewAdvisorSuggestion(suggestion)}>Preview</button><button onClick={() => { applyAdvisorSuggestions([suggestion]); setAdvisorResult((current) => current ? { ...current, suggestions: current.suggestions.filter((item) => item.id !== suggestion.id) } : current) }}>Apply</button><button onClick={() => setAdvisorResult((current) => current ? { ...current, suggestions: current.suggestions.filter((item) => item.id !== suggestion.id) } : current)}>Ignore</button></div>)}</div>}
          </div>
        </section>
        <section className="layer-stack" aria-label="Adjustment layers">
          <div className="layer-stack-head"><strong>Layers</strong><button onClick={addLayer}>+ Add</button></div>
          {selected.layers.length === 0 ? <small>No local adjustment layers</small> : selected.layers.map((layer, index) => <div className={selectedLayerId === layer.id ? 'layer-row selected' : 'layer-row'} key={layer.id} onClick={() => setSelectedLayerId(layer.id)}>
            <input aria-label={`Enable ${layer.name}`} type="checkbox" checked={layer.enabled} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, enabled: event.target.checked }))} />
            <input aria-label="Layer name" value={layer.name} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, name: event.target.value.slice(0, 80) || 'Adjustment layer' }))} />
            <label>Mask <select aria-label="Layer mask type" value={'type' in layer.mask ? layer.mask.type : 'none'} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, mask: newMaskOfType(event.target.value as 'none' | 'radial' | 'linear' | 'brush' | 'luminance' | 'colorRange') }))}><option value="none">None</option><option value="radial">Radial</option><option value="linear">Linear</option><option value="brush">Brush</option><option value="luminance">Luminance</option><option value="colorRange">Color</option><option value="portraitSemantic" disabled>Portrait (use Portrait panel)</option></select></label>
            <label>Opacity <input aria-label="Layer opacity" type="number" min="0" max="100" value={Math.round(layer.opacity * 100)} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, opacity: Math.max(0, Math.min(1, Number(event.target.value) / 100 || 0)) }))} /></label>
            <button aria-label="Move layer up" disabled={index === 0} onClick={() => moveLayer(layer.id, -1)}>↑</button><button aria-label="Move layer down" disabled={index === selected.layers.length - 1} onClick={() => moveLayer(layer.id, 1)}>↓</button>
            <button aria-label="Duplicate layer" onClick={() => duplicateLayer(layer.id)}>Copy</button><button aria-label="Delete layer" onClick={() => deleteLayer(layer.id)}>×</button>
            {selectedLayerId === layer.id && <><label>Exposure <input aria-label="Layer exposure" type="number" min="-5" max="5" step=".05" value={layer.adjustments.tone.exposureEv} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, adjustments: { tone: { ...current.adjustments.tone, exposureEv: Math.max(-5, Math.min(5, Number(event.target.value) || 0)) } } }))} /></label>
              {'type' in layer.mask && <LayerMaskControls mask={layer.mask} onChange={(mask) => updateLayer(layer.id, (current) => ({ ...current, mask }))} />}
            </>}
          </div>)}
        </section>
        <section className="layer-stack" aria-label="Reference and look workflows">
          <div className="layer-stack-head"><strong>Reference / Looks</strong><small>Native</small></div>
          <small>Perceptual reference analysis and .srlook interpolation run in Rust; no creative image math runs in React.</small>
          <div className="portrait-regions">
            <button disabled={selected.renderBackend !== 'native'} onClick={selectReference}>Select reference…</button>
            <button disabled={selected.renderBackend !== 'native'} onClick={analyzeReference}>Analyze</button>
            <button disabled={!referenceResult} onClick={previewReference}>Preview</button>
            <button disabled={!referenceResult} onClick={applyReference}>Apply</button>
            <button disabled={!referenceBase} onClick={resetReference}>Reset</button>
            <button disabled={!referenceResult} onClick={saveReferenceAsLook}>Save match as Look…</button>
            <button disabled={selected.renderBackend !== 'native'} onClick={saveLookWorkflow}>Save .srlook…</button>
            <button disabled={selected.renderBackend !== 'native'} onClick={loadLookWorkflow}>Load .srlook…</button>
          </div>
          <small>{referencePath ? `Reference: ${referencePath.split(/[\\/]/).pop()}` : 'No reference selected'}</small>
          {(Object.entries(referenceControls) as Array<[keyof typeof referenceControls, number]>).map(([key, value]) =>
            <label key={key}>{key === 'protectSkin' ? 'Protect skin' : key[0].toUpperCase() + key.slice(1)}
              <input aria-label={`Reference ${key}`} type="number" min="0" max="100" step="1" value={value}
                onChange={(event) => { setReferenceControls((controls) => ({ ...controls, [key]: Math.max(0, Math.min(100, Number(event.target.value) || 0)) })); setReferenceResult(null) }} />%</label>)}
          <label>Look amount <input aria-label="Look amount" type="number" min="0" max="100" step="1" value={lookAmount}
            onChange={(event) => setLookAmount(Math.max(0, Math.min(100, Number(event.target.value) || 0)))} />%</label>
          <div className="portrait-regions">
            <button onClick={() => selectMixerLook('a')}>Look A…</button>
            <button onClick={() => selectMixerLook('b')}>Look B…</button>
            <button disabled={!lookAPath || !lookBPath || lookAWeight + lookBWeight === 0} onClick={applyStyleMixer}>Apply A/B mix</button>
          </div>
          <small>A: {lookAPath?.split(/[\\/]/).pop() ?? 'not selected'} · B: {lookBPath?.split(/[\\/]/).pop() ?? 'not selected'}</small>
          <label>Look A weight <input aria-label="Look A weight" type="number" min="0" max="100" value={lookAWeight}
            onChange={(event) => setLookAWeight(Math.max(0, Math.min(100, Number(event.target.value) || 0)))} />%</label>
          <label>Look B weight <input aria-label="Look B weight" type="number" min="0" max="100" value={lookBWeight}
            onChange={(event) => setLookBWeight(Math.max(0, Math.min(100, Number(event.target.value) || 0)))} />%</label>
        </section>
        {view !== 'library' && selected.libraryAsset && <section className="history-panel" aria-label="Edit history and snapshots">
          <div className="layer-stack-head"><strong>History / Snapshots</strong><small>{nativeHistory?.stateVersion.slice(0, 8) ?? 'opening'}</small></div>
          <div className="snapshot-create"><input aria-label="Snapshot name" value={snapshotName} onChange={(event) => setSnapshotName(event.target.value)} /><button onClick={createSnapshot}>Save snapshot</button></div>
          <div className="history-list">{nativeHistory?.snapshots.map((snapshot) => <button key={snapshot.id} onClick={() => restoreSnapshot(snapshot.id)}><strong>{snapshot.name}</strong><small>Restore as undoable edit</small></button>)}</div>
          <div className="history-list">{nativeHistory?.entries.slice(-8).reverse().map((entry) => <div key={entry.sequence}><span>{entry.sequence}</span><strong>{entry.description}</strong><small>{entry.affectedStage}</small></div>)}</div>
        </section>}
        <div className="tool-layout">
          <nav className="tool-rail" aria-label="Editing tools">{toolItems.map(({ id, label, icon: Icon }) => <button key={id}
            className={tool === id ? 'active' : ''} aria-label={label}
            title={label} onClick={() => setTool(id)}><Icon size={18} /><span>{label}</span></button>)}</nav>
          <Inspector tool={tool} values={selected.adjustments} curvePoints={selected.curveChannels[curveChannel]} curveChannel={curveChannel} histogram={histogram} onCurveChannel={(channel) => { setCurveChannel(channel); setSelectedCurvePoint(null) }} selectedCurvePoint={selectedCurvePoint} renderBackend={selected.renderBackend}
            whiteBalanceMode={selected.whiteBalanceMode}
            mask={selected.mask} onAdjust={adjust} onBeginAdjustment={beginInteractiveEdit} onReset={resetAdjustment} onCurveSelect={setSelectedCurvePoint}
            onCurveBegin={beginInteractiveEdit} onCurveChange={updateCurve} onMaskBegin={beginInteractiveEdit} onMaskChange={updateMask}
            onCurvePresetSave={saveCurvePreset} onCurvePresetLoad={loadCurvePreset} canLoadCurvePreset={savedCurvePreset !== null}
            onWhiteBalanceMode={(mode) => updateWhiteBalance(mode)} onCopyWhiteBalance={copyWhiteBalance} onPasteWhiteBalance={pasteWhiteBalance}
            mixerBand={mixerBand} onMixerBand={setMixerBand} mixerPicking={mixerPicking} onMixerPicking={() => setMixerPicking(!mixerPicking)}
            opticsState={selected.opticsState} opticsStatus={opticsStatus} onOpticsState={updateOpticsState} onResolveOptics={refreshOpticsStatus} />
        </div>
        <button className="reset-all" disabled={!hasPhotoEdits(selected)} onClick={resetAll}><RotateCcw size={14} /> Reset all edits</button>
      </aside>
    </div>
    {notice && <button className="notice" onClick={() => setNotice('')} aria-label="Dismiss notice">{notice}</button>}
    <div className="compact-warning"><SunMedium size={18} /><span>Starroom needs a wider window for the full editing workspace.</span></div>
  </main>
}
