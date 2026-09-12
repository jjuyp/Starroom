import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { CSSProperties, MouseEvent, PointerEvent as ReactPointerEvent } from 'react'
import { selectLibraryRange } from './librarySelection'
import { loadProgressiveThumbnails } from './progressiveThumbnails'
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
  calculateHistogram, hasAdjustments, mapToneCurve,
  type RadialMask, type ToneCurvePoint,
} from './previewPresentation'
import {
  adviseNativeImage, chooseNativePhotoPaths, nativeRuntimeAvailable,
  renderNativePreview, sampleNativeColor, type NativeEditSettings, type NativePreviewResult, type NativeReferenceMatchResponse, type NativeToneCurves, type NativeWhiteBalanceMode, type NativeWhiteBalanceSample, type RenderBackend,
  defaultNativeOpticsState, resolveNativeOpticsStatus, type NativeLensIdentity, type NativeLensProfileResolution, type NativeOpticsState,
  cancelNativeAiMask, detectNativePortrait, generateNativeAiMask, defaultNativeSkinRetouch, type NativeAdjustmentLayer, type NativeAdvisorResult, type NativeAdvisorSuggestion, type NativeAiMaskResult, type NativeAiMaskSemantic, type NativeHealingOperation, type NativeMaskDefinition, type NativeMaskTree, type NativePortraitDetection, type NativePortraitRegion, type NativeSkinRetouchSettings,
  queryNativeAiAvailability, installLocalPortraitModels, type NativeAiAvailability,
  applyNativeLook, chooseNativeLookPath, chooseNativeReferencePath, fromNativeSettings, matchNativeReference, mixNativeLooks, saveNativeLook, toNativeSettings,
  addNativeLibraryCollectionAssets, addNativeLibraryKeywords, chooseNativeLibraryFolder, createNativeLibraryCollection, importNativeLibraryFolder, nativeLibraryCollectionAssets, nativeLibraryCollections, nativeLibraryThumbnail,
  openNativeLibrary, queryNativeLibrary, queryNativeLibraryIds, removeNativeLibraryAssets, removeNativeLibraryKeywords, updateNativeLibraryWorkflow,
  nativePreviewViewportContract,
  type NativeLibraryAsset, type NativeLibraryCollection, type NativeLibraryQuery, type NativeAssetFlag, type NativeColorLabel, type NativeSmartPredicate,
  commitNativeHistory, createNativeSnapshot, deleteNativeSnapshot, openNativeHistory, redoNativeHistory, renameNativeSnapshot, restoreNativeSnapshot, undoNativeHistory,
  type NativeHistoryResult,
  cancelNativeExport, chooseNativeExportDirectory, exportNativeBatch, queryNativeExportProgress, type NativeProfessionalExportSettings,
  autosaveNativeSession, discardNativeRecovery, markNativeSessionClean, openNativeSession, type NativeSessionState,
} from './nativeRender'
import { resolveCommandShortcut, searchCommands, type CommandId } from './commands'
import { formatUserError } from './errorPresentation'
import { clientPointToNormalized } from './viewportCoordinates'
import { importNativeLibraryPaths } from './nativeRender'
import { PreviewSuperseded } from './latestPreviewQueue'

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

function LibraryMetadataPanel({ asset, selectedCount, onWorkflow, onAddKeyword, onRemoveKeyword }: {
  asset: NativeLibraryAsset | null; selectedCount: number
  onWorkflow: (value: { rating?: number; flag?: NativeAssetFlag; colorLabel?: NativeColorLabel }) => void
  onAddKeyword: (keyword: string) => void
  onRemoveKeyword?: (keyword: string) => void
}) {
  const [keyword, setKeyword] = useState('')
  return <section className="library-metadata" aria-label="圖庫中繼資料">
    <div className="inspector-head"><div><span className="eyebrow">圖庫選取項目</span><h2>已選取 {selectedCount || 0} 張</h2></div></div>
    {!asset ? <div className="tool-note">請選取圖庫照片，以檢視中繼資料並批次套用工作流程欄位。</div> : <>
      <dl><dt>檔案</dt><dd>{asset.sourcePath.split(/[\\/]/).pop()}</dd><dt>類型</dt><dd>{asset.metadata.fileType.toUpperCase()}</dd>
        <dt>尺寸</dt><dd>{asset.metadata.width ?? '—'} × {asset.metadata.height ?? '—'}</dd>
        <dt>相機</dt><dd>{[asset.metadata.cameraMake, asset.metadata.cameraModel].filter(Boolean).join(' ') || '—'}</dd>
        <dt>鏡頭</dt><dd>{[asset.metadata.lensMake, asset.metadata.lensModel].filter(Boolean).join(' ') || '—'}</dd>
        <dt>ISO</dt><dd>{asset.metadata.iso ?? '—'}</dd><dt>狀態</dt><dd>{asset.missing ? '找不到來源' : '可用'}</dd></dl>
      <label>評分<select value={asset.rating} onChange={(event) => onWorkflow({ rating: Number(event.target.value) })}>{[0,1,2,3,4,5].map((value) => <option key={value} value={value}>{value ? `${value} 星` : '未評分'}</option>)}</select></label>
      <label>旗標<select value={asset.flag} onChange={(event) => onWorkflow({ flag: event.target.value as NativeAssetFlag })}><option value="unflagged">無旗標</option><option value="pick">保留</option><option value="reject">拒絕</option></select></label>
      <label>顏色標籤<select value={asset.colorLabel} onChange={(event) => onWorkflow({ colorLabel: event.target.value as NativeColorLabel })}>{([['none','無'],['red','紅色'],['yellow','黃色'],['green','綠色'],['blue','藍色'],['purple','紫色']] as const).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      <div className="keyword-editor"><span>{asset.keywords.length ? asset.keywords.map((name) => <button key={name} title="從選取項目移除關鍵字" onClick={() => onRemoveKeyword?.(name)}>{name} ×</button>) : '沒有關鍵字'}</span><input value={keyword} placeholder="加入關鍵字" onChange={(event) => setKeyword(event.target.value)} /><button onClick={() => { if (keyword.trim()) { onAddKeyword(keyword); setKeyword('') } }}>加入</button></div>
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
      <button type="button" disabled={mask.points.length <= 1} onClick={() => onChange({ ...mask, points: mask.points.slice(0, -1) })}>移除控制點</button>
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
  { id: 'light', label: '光線', icon: SunMedium },
  { id: 'color', label: '色彩', icon: Blend },
  { id: 'curve', label: '曲線', icon: ScanLine },
  { id: 'detail', label: '細節', icon: Aperture },
  { id: 'looks', label: '風格', icon: Sparkles },
  { id: 'masks', label: '遮罩', icon: ScanFace },
  { id: 'heal', label: '修復', icon: Sparkles },
  { id: 'optics', label: '鏡頭', icon: Contrast },
  { id: 'geometry', label: '幾何', icon: Crop },
]

const sliderGroups: Partial<Record<Tool, Array<{ key: AdjustmentKey; label: string; min: number; max: number; step: number; suffix?: string }>>> = {
  light: [
    { key: 'exposure', label: '曝光', min: -5, max: 5, step: .01, suffix: ' EV' },
    { key: 'contrast', label: '對比', min: -100, max: 100, step: 1 },
    { key: 'highlights', label: '高光', min: -100, max: 100, step: 1 },
    { key: 'shadows', label: '陰影', min: -100, max: 100, step: 1 },
    { key: 'whites', label: '白色', min: -100, max: 100, step: 1 },
    { key: 'blacks', label: '黑色', min: -100, max: 100, step: 1 },
  ],
  color: [
    { key: 'temperature', label: '色溫', min: -100, max: 100, step: 1 },
    { key: 'tint', label: '色調', min: -100, max: 100, step: 1 },
    { key: 'vibrance', label: '自然飽和度', min: -100, max: 100, step: 1 },
    { key: 'saturation', label: '飽和度', min: -100, max: 100, step: 1 },
  ],
  detail: [
    { key: 'sharpness', label: '銳利化強度', min: 0, max: 100, step: 1 },
    { key: 'sharpenRadius', label: '銳利化半徑', min: .3, max: 4, step: .1, suffix: ' px' },
    { key: 'sharpenDetail', label: '銳利化細節', min: 0, max: 100, step: 1 },
    { key: 'sharpenMasking', label: '銳利化遮罩', min: 0, max: 100, step: 1 },
    { key: 'sharpenHaloProtection', label: '光暈保護', min: 0, max: 100, step: 1 },
    { key: 'texture', label: '紋理', min: -100, max: 100, step: 1 },
    { key: 'clarity', label: '清晰度', min: -100, max: 100, step: 1 },
    { key: 'dehaze', label: '去朦朧', min: -100, max: 100, step: 1 },
    { key: 'denoiseLuminance', label: '明度降噪', min: 0, max: 100, step: 1 },
    { key: 'denoiseChroma', label: '色彩降噪', min: 0, max: 100, step: 1 },
    { key: 'denoiseRadius', label: '降噪半徑', min: .6, max: 4, step: .1, suffix: ' px' },
    { key: 'denoiseDetailProtection', label: '細節保護', min: 0, max: 100, step: 1 },
    { key: 'denoiseHighIso', label: '高 ISO', min: 0, max: 100, step: 1 },
    { key: 'aiDenoiseEnabled', label: '啟用 AI 降噪', min: 0, max: 1, step: 1 },
    { key: 'aiDenoiseAmount', label: 'AI 降噪強度', min: 0, max: 100, step: 1 },
    { key: 'aiDenoiseDetail', label: 'AI 細節保留', min: 0, max: 100, step: 1 },
    { key: 'aiDenoiseColorNoise', label: 'AI 色彩雜訊', min: 0, max: 100, step: 1 },
    { key: 'aiDenoisePreserveSkin', label: 'AI 肌膚保護', min: 0, max: 100, step: 1 },
  ],
  looks: [
    { key: 'grainAmount', label: '顆粒強度', min: 0, max: 100, step: 1 },
    { key: 'grainSize', label: '顆粒大小', min: 10, max: 100, step: 1 },
    { key: 'grainRoughness', label: '顆粒粗糙度', min: 0, max: 100, step: 1 },
    { key: 'grainColor', label: '彩色顆粒', min: 0, max: 100, step: 1 },
    { key: 'vignette', label: '暗角強度', min: -100, max: 100, step: 1 },
    { key: 'vignetteMidpoint', label: '暗角中點', min: 0, max: 100, step: 1 },
    { key: 'vignetteRoundness', label: '暗角圓度', min: -100, max: 100, step: 1 },
    { key: 'vignetteFeather', label: '暗角羽化', min: 2, max: 100, step: 1 },
    { key: 'vignetteHighlightProtect', label: '高光保護', min: 0, max: 100, step: 1 },
  ],
  masks: [
    { key: 'maskExposure', label: '中心曝光', min: -3, max: 3, step: .01, suffix: ' EV' },
    { key: 'maskFeather', label: '羽化', min: 0, max: 100, step: 1 },
  ],
  geometry: [
    { key: 'geometryScale', label: '縮放', min: 5, max: 200, step: .1, suffix: '%' },
    { key: 'geometryOffsetX', label: '水平位移', min: -100, max: 100, step: .1, suffix: '%' },
    { key: 'geometryOffsetY', label: '垂直位移', min: -100, max: 100, step: .1, suffix: '%' },
    { key: 'geometryVertical', label: '垂直透視', min: -100, max: 100, step: .1 },
    { key: 'geometryHorizontal', label: '水平透視', min: -100, max: 100, step: .1 },
    { key: 'cropLeft', label: '左側裁切', min: 0, max: 99, step: .1, suffix: '%' },
    { key: 'cropTop', label: '上方裁切', min: 0, max: 99, step: .1, suffix: '%' },
    { key: 'cropRight', label: '右側裁切', min: 1, max: 100, step: .1, suffix: '%' },
    { key: 'cropBottom', label: '下方裁切', min: 1, max: 100, step: .1, suffix: '%' },
    { key: 'rotation', label: '旋轉角度', min: -180, max: 180, step: .1, suffix: '°' },
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
  return <div className="slider-row" data-control={label.toLowerCase()}>
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
    return clientPointToNormalized(event, event.currentTarget.getBoundingClientRect(), true)
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
    <div className="curve-presets"><button onClick={() => { onBeginEdit(); onChange(copyCurve(defaultCurvePoints)) }}>線性</button><button onClick={() => { onBeginEdit(); onChange([{ id: 'black', x: 0, y: 0 }, { id: 'shadow', x: .25, y: .18 }, { id: 'midtone', x: .5, y: .5 }, { id: 'highlight', x: .75, y: .84 }, { id: 'white', x: 1, y: 1 }]) }}>S 曲線</button><button onClick={() => { onBeginEdit(); onChange([{ id: 'black', x: 0, y: .10 }, { id: 'midtone', x: .5, y: .55 }, { id: 'white', x: 1, y: 1 }]) }}>黑色淡化</button></div>
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
    <div className="curve-help">單調曲線 · 左鍵點線新增控制點 · 拖曳調整 · 右鍵刪除</div>
    {selected && <div className="curve-values">
      <label>輸入 <input aria-label="所選曲線控制點輸入值" type="number" min="0" max="100" step="1" value={Math.round(selected.x * 100)}
        disabled={selected.id === 'black' || selected.id === 'white'} onFocus={onBeginEdit}
        onChange={(event) => updatePoint(selected.id, { x: Number(event.target.value) / 100 })} /></label>
      <label>輸出 <input aria-label="所選曲線控制點輸出值" type="number" min="0" max="100" step="1" value={Math.round(selected.y * 100)}
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
    return clientPointToNormalized(event, event.currentTarget.getBoundingClientRect())
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

function PreviewCanvas({ photo, before, zoom, zoomScale = 1, pan = { x: 0, y: 0 }, interactionPhase = 'final', maskActive = false, healActive = false, brushActive = false, maskPreview = null, onBeginMaskEdit, onMaskChange, onHealingStroke, onBrushStroke, onWhiteBalancePick, onColorSample, onHistogram, onStatus, onDimensions, onDisplayScale, metric = true }: {
  photo: PhotoItem; before: boolean; zoom: 'fit' | '100'
  zoomScale?: number
  pan?: { x: number; y: number }
  interactionPhase?: 'interactive' | 'final'
  maskActive?: boolean; onBeginMaskEdit?: () => void; onMaskChange?: (mask: RadialMask) => void
  healActive?: boolean; onHealingStroke?: (points: Array<{ x: number; y: number }>) => void
  brushActive?: boolean; onBrushStroke?: (points: Array<{ x: number; y: number }>) => void
  maskPreview?: NativeMaskTree | null
  onWhiteBalancePick?: (sample: NativeWhiteBalanceSample) => void
  onColorSample?: (x: number, y: number) => void
  onHistogram: (values: number[]) => void
  onStatus: (status: string) => void
  onDimensions: (dimensions: string) => void
  onDisplayScale?: (scale: number) => void
  metric?: boolean
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const tileCanvasRef = useRef<HTMLCanvasElement>(null)
  const previewSurface = useRef({})
  const displayedSource = useRef('')
  const activePhotoId = useRef(photo.id)
  const nativeDimensions = useRef({ id: '', width: 0, height: 0 })
  const [sourceDisplaySize, setSourceDisplaySize] = useState({ width: 0, height: 0 })
  const [aspect, setAspect] = useState(1.5)
  const [surfaceSize, setSurfaceSize] = useState({ width: 0, height: 0 })
  const [canvasBounds, setCanvasBounds] = useState({ left: 0, top: 0, width: 0, height: 0 })
  const [tileRegion, setTileRegion] = useState<{ x: number; y: number; width: number; height: number } | null>(null)
  const healingStroke = useRef<Array<{ x: number; y: number }> | null>(null)
  const maskBrushStroke = useRef<Array<{ x: number; y: number }> | null>(null)
  const healPoint = (event: React.PointerEvent<HTMLCanvasElement>) => {
    return clientPointToNormalized(event, event.currentTarget.getBoundingClientRect())
  }

  useEffect(() => { activePhotoId.current = photo.id }, [photo.id])

  useEffect(() => {
    let finalPublished = false
    if (photo.renderBackend === 'native' && photo.src && displayedSource.current !== photo.id) {
      const cachedThumbnail = new Image()
      cachedThumbnail.onload = () => {
        if (activePhotoId.current !== photo.id || finalPublished || !canvasRef.current || displayedSource.current === photo.id) return
        const canvas = canvasRef.current
        const context = canvas.getContext('2d')
        if (!context) return
        canvas.width = cachedThumbnail.naturalWidth
        canvas.height = cachedThumbnail.naturalHeight
        context.drawImage(cachedThumbnail, 0, 0)
        setAspect(cachedThumbnail.naturalWidth / cachedThumbnail.naturalHeight)
        setTileRegion(null)
        onDimensions(`${photo.libraryAsset?.metadata.width ?? cachedThumbnail.naturalWidth} × ${photo.libraryAsset?.metadata.height ?? cachedThumbnail.naturalHeight}`)
        onStatus('原生快取縮圖 · 正在精細化…')
      }
      cachedThumbnail.src = photo.src
    }
    const timeout = window.setTimeout(async () => {
      onStatus('正在算圖…')
      try {
        const adjustments = before ? defaultAdjustments : photo.adjustments
        const curvePoints = before ? defaultCurvePoints : photo.curvePoints
        const mask = before ? defaultMask : photo.mask
        let rendered: CanvasImageSource
        let renderedWidth: number
        let renderedHeight: number
        let nativeProfile = ''
        let nativeAcceleration: 'gpu' | 'cpuFallback' = 'cpuFallback'
        let nativeResult: NativePreviewResult | null = null
        let release: (() => void) | undefined
        if (photo.renderBackend === 'native') {
          if (!photo.sourcePath) throw new Error('Native photo is missing its source path; Browser fallback was not used.')
          const previewLayers = maskPreview && !before ? [
            ...photo.layers,
            { ...defaultLayer(), id: '__mask-preview-dim__', name: 'Mask preview outside', opacity: .72, mask: { operation: 'invert' as const, children: [structuredClone(maskPreview)] }, adjustments: { tone: { ...defaultLayer().adjustments.tone, exposureEv: -1.35 } } },
            { ...defaultLayer(), id: '__mask-preview-inside__', name: 'Mask preview inside', opacity: .38, mask: structuredClone(maskPreview), adjustments: { tone: { ...defaultLayer().adjustments.tone, exposureEv: .55 } } },
          ] : photo.layers
          const sourceWidth = nativeDimensions.current.id === photo.id ? nativeDimensions.current.width : photo.libraryAsset?.metadata.width ?? 0
          const sourceHeight = nativeDimensions.current.id === photo.id ? nativeDimensions.current.height : photo.libraryAsset?.metadata.height ?? 0
          const baseRect = canvasRef.current?.getBoundingClientRect()
          const stageRect = canvasRef.current?.closest('.photo-stage')?.getBoundingClientRect()
          const visible = baseRect && stageRect && baseRect.width > 0 && baseRect.height > 0 ? {
            centerX: Math.max(0, Math.min(1, (stageRect.left + stageRect.width / 2 - baseRect.left) / baseRect.width)),
            centerY: Math.max(0, Math.min(1, (stageRect.top + stageRect.height / 2 - baseRect.top) / baseRect.height)),
            widthFraction: Math.max(0, Math.min(1, stageRect.width / baseRect.width)),
            heightFraction: Math.max(0, Math.min(1, stageRect.height / baseRect.height)),
          } : undefined
          const displayEdge = Math.max(baseRect?.width ?? 0, baseRect?.height ?? 0)
            * Math.max(1, window.devicePixelRatio || 1)
          const viewport = nativePreviewViewportContract(zoom, zoomScale, sourceWidth, sourceHeight, visible, displayEdge)
          const result = await renderNativePreview(photo.sourcePath, adjustments, curvePoints, mask,
            before ? 'sourceDefault' : photo.whiteBalanceMode, before ? null : photo.whiteBalanceSample,
            before ? defaultCurveChannels() : photo.curveChannels, before ? defaultNativeOpticsState : photo.opticsState,
            before ? [] : previewLayers, viewport.maxEdge, before ? defaultNativeSkinRetouch() : photo.skinRetouch, before ? [] : photo.healingOperations, interactionPhase, previewSurface.current,
            viewport.resolutionMode, viewport.viewport)
          nativeResult = result
          if (activePhotoId.current === photo.id) {
            nativeDimensions.current = { id: photo.id, width: result.sourceWidth, height: result.sourceHeight }
            setSourceDisplaySize({ width: result.sourceWidth, height: result.sourceHeight })
          }
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
          const original = new Image()
          await new Promise<void>((resolve, reject) => {
            original.onload = () => resolve()
            original.onerror = () => reject(new Error('Browser image could not be decoded.'))
            original.src = photo.src
          })
          rendered = original
          renderedWidth = original.naturalWidth
          renderedHeight = original.naturalHeight
          onStatus('僅顯示原圖 · 編輯需要原生桌面版')
        }
        if (activePhotoId.current !== photo.id || !canvasRef.current) {
          release?.()
          return
        }
        finalPublished = true
        const canvas = canvasRef.current
        const context = canvas.getContext('2d', { willReadFrequently: true })
        if (!context) {
          release?.()
          throw new Error('Canvas 2D is unavailable.')
        }
        if (nativeResult?.isTile) {
          const tileCanvas = tileCanvasRef.current
          const tileContext = tileCanvas?.getContext('2d')
          if (!tileCanvas || !tileContext) throw new Error('Native viewport tile canvas is unavailable.')
          tileCanvas.width = renderedWidth
          tileCanvas.height = renderedHeight
          tileContext.drawImage(rendered, 0, 0)
          setTileRegion({
            x: nativeResult.tileX / nativeResult.sourceWidth,
            y: nativeResult.tileY / nativeResult.sourceHeight,
            width: nativeResult.width / nativeResult.sourceWidth,
            height: nativeResult.height / nativeResult.sourceHeight,
          })
        } else {
          canvas.width = renderedWidth
          canvas.height = renderedHeight
          context.drawImage(rendered, 0, 0)
          displayedSource.current = photo.id
          setAspect(renderedWidth / renderedHeight)
          setTileRegion(null)
        }
        release?.()
        window.requestAnimationFrame(() => {
          if (!canvasRef.current) return
          setCanvasBounds({ left: canvasRef.current.offsetLeft, top: canvasRef.current.offsetTop,
            width: canvasRef.current.clientWidth, height: canvasRef.current.clientHeight })
          const sourceWidth = nativeResult?.sourceWidth ?? photo.libraryAsset?.metadata.width ?? renderedWidth
          if (sourceWidth > 0) onDisplayScale?.(canvasRef.current.clientWidth / sourceWidth)
        })
        if (metric) {
          if (!nativeResult?.isTile) onHistogram(calculateHistogram(context.getImageData(0, 0, canvas.width, canvas.height)))
          onDimensions(nativeResult ? `${nativeResult.sourceWidth} × ${nativeResult.sourceHeight}` : `${renderedWidth} × ${renderedHeight}`)
          onStatus(photo.renderBackend === 'native'
            ? `${nativeAcceleration === 'gpu' ? '原生 GPU' : '原生 CPU 備援'} · ${nativeProfile}${interactionPhase === 'interactive' ? ' · 即時預覽 1024' : nativeResult?.isTile ? nativeResult.tileOptimized ? ' · 可視區域圖塊' : ' · 可視區域圖塊 · 全幅相容' : ' · 最終品質'}${before ? ' · 原圖' : ''}`
            : `瀏覽器備援${before ? ' · 原圖' : ''}`)
        }
      } catch (error) {
        if (activePhotoId.current === photo.id && !(error instanceof PreviewSuperseded)) onStatus(formatUserError(error, '預覽失敗'))
      }
    }, 30)

    return () => {
      window.clearTimeout(timeout)
    }
  }, [before, metric, onDimensions, onHistogram, onStatus, photo.adjustments, photo.curvePoints, photo.curveChannels, photo.whiteBalanceMode, photo.whiteBalanceSample,
    photo.mask, photo.opticsState, photo.layers, photo.skinRetouch, photo.healingOperations, photo.renderBackend, photo.sourcePath, photo.src,
    photo.libraryAsset?.metadata.width, photo.libraryAsset?.metadata.height,
    maskPreview, interactionPhase, zoom, zoomScale, pan.x, pan.y, onDisplayScale, photo.id])

  useEffect(() => {
    const measure = () => canvasRef.current && setCanvasBounds({ left: canvasRef.current.offsetLeft, top: canvasRef.current.offsetTop,
      width: canvasRef.current.clientWidth, height: canvasRef.current.clientHeight })
    const observer = new ResizeObserver(measure)
    if (canvasRef.current) observer.observe(canvasRef.current)
    window.addEventListener('resize', measure)
    return () => { observer.disconnect(); window.removeEventListener('resize', measure) }
  }, [])

  useEffect(() => {
    const parent = canvasRef.current?.parentElement
    if (!parent) return
    const observer = new ResizeObserver(() => setSurfaceSize({ width: parent.clientWidth, height: parent.clientHeight }))
    observer.observe(parent)
    return () => observer.disconnect()
  }, [])
  const fitWidth = Math.min(surfaceSize.width, surfaceSize.height * aspect)

  return <>
    <canvas ref={canvasRef} className={`photo-canvas zoom-${zoom}`} aria-label={`Edited preview of ${photo.name}`}
      style={zoom === '100' && sourceDisplaySize.width ? {
        width: `${sourceDisplaySize.width}px`, height: `${sourceDisplaySize.height}px`,
      } : fitWidth > 0 ? { width: `${fitWidth}px`, height: `${fitWidth / aspect}px` } : undefined}
      onPointerDown={(event) => { if (before || event.button !== 0 || (!healActive && !brushActive)) return; const points = [healPoint(event)]; if (healActive) healingStroke.current = points; else maskBrushStroke.current = points; event.currentTarget.setPointerCapture(event.pointerId) }}
      onPointerMove={(event) => { const points = healingStroke.current ?? maskBrushStroke.current; if (!points) return; const point = healPoint(event); const previous = points.at(-1)!; if (Math.hypot(point.x - previous.x, point.y - previous.y) >= .004) points.push(point) }}
      onPointerUp={(event) => { const healing = healingStroke.current; const brushing = maskBrushStroke.current; healingStroke.current = null; maskBrushStroke.current = null; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); if (healing?.length) onHealingStroke?.(healing); if (brushing?.length) onBrushStroke?.(brushing) }}
      onDoubleClick={(event) => {
        if (before) return
        const { x: pointX, y: pointY } = clientPointToNormalized(event, event.currentTarget.getBoundingClientRect())
        if (onColorSample) { onColorSample(pointX, pointY); return }
        if (photo.whiteBalanceMode !== 'neutralPicker' || !onWhiteBalancePick) return
        const size = .06
        const x = Math.max(0, Math.min(1 - size, pointX - size / 2))
        const y = Math.max(0, Math.min(1 - size, pointY - size / 2))
        onWhiteBalancePick({ x, y, width: size, height: size })
      }} />
    <canvas ref={tileCanvasRef} className="photo-tile-canvas" aria-hidden="true" style={tileRegion ? {
      left: `${canvasBounds.left + tileRegion.x * canvasBounds.width}px`,
      top: `${canvasBounds.top + tileRegion.y * canvasBounds.height}px`,
      width: `${tileRegion.width * canvasBounds.width}px`,
      height: `${tileRegion.height * canvasBounds.height}px`,
    } : { display: 'none' }} />
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
    <div className="inspector-head"><div><span className="eyebrow">調整</span><h2>{toolItems.find((item) => item.id === tool)?.label ?? tool}</h2></div><ChevronDown size={16} /></div>
    {renderBackend === 'native' && tool === 'masks'
      && <div className="tool-note">原生 M15 遮罩圖層：虛線放射狀選取範圍會在預覽、編輯前後比較與匯出的共用管線中運算，不使用瀏覽器 Canvas 合成。</div>}
    {tool === 'color' && <>
      <div className="tool-note">一般影像的色溫／色調為相對校正，不代表物理 Kelvin 值；RAW 的相機／拍攝時設定會使用 LibRaw 中繼資料。</div>
      {renderBackend === 'native' && <div className="wb-controls"><label>白平衡模式<select value={whiteBalanceMode}
        onFocus={onBeginAdjustment} onChange={(event) => onWhiteBalanceMode(event.target.value as NativeWhiteBalanceMode)}>
        <option value="sourceDefault">來源預設</option><option value="asShot">拍攝時設定（RAW）</option>
        <option value="camera">相機白平衡（RAW）</option><option value="auto">自動（灰世界）</option>
        <option value="neutralPicker">中性灰吸管</option><option value="relative">相對校正（一般影像）</option>
      </select></label><div><button onClick={onCopyWhiteBalance}>複製白平衡</button><button onClick={onPasteWhiteBalance}>貼上白平衡</button></div>
      <small>{whiteBalanceMode === 'neutralPicker' ? 'Double-click a neutral area in the preview to sample it.' : 'Mode is recorded with this non-destructive edit.'}</small></div>}
      <div className="basic-color-controls"><strong>基本色彩</strong>
        {sliders.map(({ key, ...slider }) => <Slider key={key} {...slider} value={values[key]} onBeginEdit={onBeginAdjustment}
          onChange={(value) => onAdjust(key, value, false)} onReset={() => onReset(key)} />)}
      </div>
      <div className="mixer-panel" aria-label="Eight-band Color Mixer">
        <div className="mixer-heading"><strong>色彩混合器</strong><button className={mixerPicking ? 'active' : ''} onClick={onMixerPicking}>目標調整</button><label><input type="checkbox" checked={values.mixerHueLock !== 0}
          onFocus={onBeginAdjustment} onChange={(event) => onAdjust('mixerHueLock', event.target.checked ? 1 : 0)} /> 鎖定色相</label></div>
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
        <small>目標式調整使用原生 OKLCh 運算，色相範圍會環狀重疊以保持平順。</small>
      </div>
      <div className="grading-panel" aria-label="Four-way Color Grading">
        <div className="mixer-heading"><strong>色彩分級</strong><small>原生 OKLab</small></div>
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
    {tool === 'masks' && <div className="tool-note">點擊照片放置遮罩；拖曳內部可移動，拖曳側邊控制點可調整大小，拖曳上方控制點可旋轉。</div>}
    {tool === 'curve' && <><CurveChannelTabs value={curveChannel} onChange={onCurveChannel} /><ToneCurveEditor points={curvePoints} selectedId={selectedCurvePoint} onSelect={onCurveSelect}
      histogram={histogram} onBeginEdit={onCurveBegin} onChange={onCurveChange} /><div className="curve-presets"><button onClick={onCurvePresetSave}>儲存自訂曲線</button><button disabled={!canLoadCurvePreset} onClick={onCurvePresetLoad}>載入自訂曲線</button></div></>}
    {tool === 'optics' && <div className="optics-controls">
      <div className="tool-note">Lensfun v0.3.4 鏡頭描述檔校正。缺少、模糊或不相符的中繼資料都會明確提示。</div>
      <label><input type="checkbox" checked={values.lensCorrection !== 0} onChange={(event) => onAdjust('lensCorrection', event.target.checked ? 1 : 0)} /> 啟用 Lensfun 校正</label>
      <label><input type="checkbox" checked={values.lensDistortion !== 0} onChange={(event) => onAdjust('lensDistortion', event.target.checked ? 1 : 0)} /> 變形校正</label>
      <label><input type="checkbox" checked={values.lensTca !== 0} onChange={(event) => onAdjust('lensTca', event.target.checked ? 1 : 0)} /> TCA</label>
      <label><input type="checkbox" checked={values.lensVignette !== 0} onChange={(event) => onAdjust('lensVignette', event.target.checked ? 1 : 0)} /> Vignette</label>
      <label><input type="checkbox" checked={values.lensAutoScale !== 0} onChange={(event) => onAdjust('lensAutoScale', event.target.checked ? 1 : 0)} /> Auto scale</label>
      <label>比對模式<select value={opticsState.matchMode} onChange={(event) => onOpticsState({ ...opticsState, matchMode: event.target.value as 'auto' | 'manual',
        manualIdentity: event.target.value === 'manual' ? opticsState.manualIdentity ?? { cameraMake: '', cameraModel: '', lensMake: '', lensModel: '', focalLengthMm: 0, aperture: 0, focusDistanceM: null } : null })}>
        <option value="auto">自動使用中繼資料</option><option value="manual">手動選擇描述檔</option></select></label>
      {opticsState.matchMode === 'manual' && <div className="optics-manual">{([
        ['Camera make', 'cameraMake'], ['Camera model', 'cameraModel'], ['Lens make', 'lensMake'], ['Lens model', 'lensModel'],
      ] as const).map(([label, key]) => <label key={key}>{label}<input value={opticsState.manualIdentity?.[key] ?? ''}
        onChange={(event) => onOpticsState({ ...opticsState, manualIdentity: { ...(opticsState.manualIdentity as NativeLensIdentity), [key]: event.target.value } })} /></label>)}
        {([['Focal mm', 'focalLengthMm'], ['Aperture', 'aperture'], ['Focus m', 'focusDistanceM']] as const).map(([label, key]) => <label key={key}>{label}<input type="number" min="0" step="0.1"
          value={opticsState.manualIdentity?.[key] ?? ''} onChange={(event) => onOpticsState({ ...opticsState,
            manualIdentity: { ...(opticsState.manualIdentity as NativeLensIdentity), [key]: event.target.value === '' ? (key === 'focusDistanceM' ? null : 0) : Number(event.target.value) } })} /></label>)}</div>}
      <button onClick={onResolveOptics}>解析 Lensfun 描述檔</button>
      <div className={`optics-status status-${opticsStatus?.status ?? 'idle'}`}><strong>{opticsStatus?.status ?? 'Not resolved'}</strong>
        <span>{opticsStatus?.profileId ?? 'No profile selected'}</span><small>{opticsStatus?.cameraMount ?? ''} · DB {opticsStatus?.databaseVersion ?? '0.3.4'}</small></div>
    </div>}
    {tool !== 'color' && sliders.map(({ key, ...slider }) => <Slider key={key} {...slider} value={values[key]} onBeginEdit={onBeginAdjustment}
      onChange={(value) => onAdjust(key, value, false)} onReset={() => onReset(key)} />)}
    {tool === 'masks' && <div className="mask-values">
      {([
        ['中心 X', 'x', mask.x * 100, 0, 100, '%'], ['中心 Y', 'y', mask.y * 100, 0, 100, '%'],
        ['寬度', 'width', mask.width * 100, 4, 160, '%'], ['高度', 'height', mask.height * 100, 4, 160, '%'],
        ['角度', 'rotation', mask.rotation, -180, 180, '°'],
      ] as const).map(([label, key, value, min, max, suffix]) => <label key={key}>{label}<span><input aria-label={`Mask ${label} value`} type="number"
        min={min} max={max} step={key === 'rotation' ? .1 : 1} value={Math.round(value * 10) / 10}
        onFocus={onMaskBegin} onChange={(event) => {
          const next = Math.min(max, Math.max(min, Number(event.target.value)))
          onMaskChange({ ...mask, [key]: key === 'rotation' ? next : next / 100 })
        }} />{suffix}</span></label>)}
    </div>}
    {tool === 'geometry' && <div className="geometry-controls">
      <label>自動校正<select value={Math.round(values.geometryUpright)} onChange={(event) => onAdjust('geometryUpright', Number(event.target.value))}>
        <option value="0">關閉</option><option value="1">自動</option><option value="2">水平</option><option value="3">垂直</option><option value="4">完整</option></select></label>
      <div className="aspect-presets"><button onClick={() => { onBeginAdjustment(); onAdjust('cropAspectWidth', 0, false); onAdjust('cropAspectHeight', 0, false) }}>自由</button>
        <button onClick={() => { onBeginAdjustment(); onAdjust('cropAspectWidth', -1, false); onAdjust('cropAspectHeight', -1, false) }}>原始比例</button>
        {([[1,1,'1:1'],[4,3,'4:3'],[3,2,'3:2'],[16,9,'16:9']] as const).map(([width,height,label]) => <button key={label}
          onClick={() => { onBeginAdjustment(); onAdjust('cropAspectWidth', width, false); onAdjust('cropAspectHeight', height, false) }}>{label}</button>)}</div>
      <button onClick={() => onAdjust('rotation', normalizeAngle(values.rotation - 90))}><RotateCcw size={16} /> 向左旋轉</button>
      <button onClick={() => onAdjust('rotation', normalizeAngle(values.rotation + 90))}><RotateCw size={16} /> 向右旋轉</button>
      <button className={values.flipHorizontal ? 'active' : ''} onClick={() => onAdjust('flipHorizontal', values.flipHorizontal ? 0 : 1)}><FlipHorizontal2 size={16} /> Flip horizontal</button>
      <button className={values.flipVertical ? 'active' : ''} onClick={() => onAdjust('flipVertical', values.flipVertical ? 0 : 1)}><FlipVertical2 size={16} /> Flip vertical</button>
      <button className={values.geometryFourPoint ? 'active' : ''} onClick={() => onAdjust('geometryFourPoint', values.geometryFourPoint ? 0 : 1)}>四點透視</button>
      {values.geometryFourPoint !== 0 && <div className="quad-values">{([
        ['TL X','quadTopLeftX'],['TL Y','quadTopLeftY'],['TR X','quadTopRightX'],['TR Y','quadTopRightY'],
        ['BR X','quadBottomRightX'],['BR Y','quadBottomRightY'],['BL X','quadBottomLeftX'],['BL Y','quadBottomLeftY'],
      ] as const).map(([label,key]) => <label key={key}>{label}<input type="number" min="0" max="100" step="0.1" value={values[key]}
        onFocus={onBeginAdjustment} onChange={(event) => onAdjust(key, Math.min(100, Math.max(0, Number(event.target.value))), false)} />%</label>)}</div>}
    </div>}
    <div className="intent-card"><Sparkles size={17} /><div><strong>非破壞性編輯</strong><span>可直接輸入數值；雙擊滑桿可重設</span></div></div>
  </section>
}

function ExportPanel({ settings, busy, selectedCount, nativeAvailable, onChange, onExport, onCancel }: {
  settings: NativeProfessionalExportSettings; busy: boolean; selectedCount: number; nativeAvailable: boolean
  onChange: (settings: NativeProfessionalExportSettings) => void; onExport: () => void; onCancel: () => void
}) {
  const resizePixels = 'pixels' in settings.resize ? settings.resize.pixels : 2048
  return <section className="export-panel" aria-label="專業匯出">
    <div className="layer-stack-head"><strong>專業匯出</strong><small>{selectedCount > 1 ? `已選取 ${selectedCount} 張` : '完整解析度'}</small></div>
    <small>共用原生處理管線 · 瀏覽器備援會明確提示 · 原始檔保持不變</small>
    <div className="export-grid">
      <label>格式<select aria-label="匯出格式" value={settings.format} onChange={(event) => { const format = event.target.value as NativeProfessionalExportSettings['format']; onChange({ ...settings, format, bitDepth: format === 'jpeg' ? 8 : settings.bitDepth }) }}><option value="jpeg">JPEG</option><option value="png">PNG</option><option value="tiff">TIFF</option></select></label>
      <label>位元深度<select aria-label="匯出位元深度" value={settings.bitDepth} onChange={(event) => onChange({ ...settings, bitDepth: Number(event.target.value) as 8 | 16 })}><option value="8">8 位元</option><option value="16" disabled={settings.format === 'jpeg'}>16 位元</option></select></label>
      {settings.format === 'jpeg' && <label>品質<input aria-label="匯出品質" type="number" min="1" max="100" value={settings.quality} onChange={(event) => onChange({ ...settings, quality: Math.max(1, Math.min(100, Number(event.target.value) || 1)) })} /></label>}
      <label>色彩空間<select aria-label="匯出色彩空間" value={settings.colorSpace} onChange={(event) => onChange({ ...settings, colorSpace: event.target.value as NativeProfessionalExportSettings['colorSpace'] })}><option value="srgb">sRGB</option><option value="displayP3">Display P3</option><option value="adobeRgb">Adobe RGB</option><option value="rec2020">Rec.2020</option></select></label>
      <label>調整尺寸<select aria-label="匯出尺寸模式" value={settings.resize.mode} onChange={(event) => { const mode = event.target.value; onChange({ ...settings, resize: mode === 'original' ? { mode } : { mode: mode as 'width' | 'height' | 'longEdge' | 'shortEdge', pixels: resizePixels } }) }}><option value="original">原始尺寸</option><option value="width">寬度</option><option value="height">高度</option><option value="longEdge">長邊</option><option value="shortEdge">短邊</option></select></label>
      {settings.resize.mode !== 'original' && 'pixels' in settings.resize && <label>像素<input aria-label="匯出像素尺寸" type="number" min="1" value={settings.resize.pixels} onChange={(event) => onChange({ ...settings, resize: { mode: settings.resize.mode as 'width' | 'height' | 'longEdge' | 'shortEdge', pixels: Math.max(1, Number(event.target.value) || 1) } })} /></label>}
      <label>輸出銳利化<select aria-label="輸出銳利化" value={settings.outputSharpen} onChange={(event) => onChange({ ...settings, outputSharpen: event.target.value as 'off' | 'screen' | 'print' })}><option value="off">關閉</option><option value="screen">螢幕</option><option value="print">列印</option></select></label>
      <label>強度<select aria-label="輸出銳利化強度" value={settings.sharpenAmount} disabled={settings.outputSharpen === 'off'} onChange={(event) => onChange({ ...settings, sharpenAmount: event.target.value as 'low' | 'standard' | 'high' })}><option value="low">低</option><option value="standard">標準</option><option value="high">高</option></select></label>
      <label>中繼資料<select aria-label="匯出中繼資料政策" value={settings.metadata} onChange={(event) => onChange({ ...settings, metadata: event.target.value as NativeProfessionalExportSettings['metadata'] })}><option value="allMetadata">所有安全的中繼資料</option><option value="cameraMetadata">僅相機資料</option><option value="copyrightOnly">僅版權資料</option><option value="none">不包含</option></select></label>
      <label>檔名衝突<select aria-label="匯出檔名衝突政策" value={settings.collision} onChange={(event) => onChange({ ...settings, collision: event.target.value as NativeProfessionalExportSettings['collision'] })}><option value="autoRename">自動重新命名</option><option value="fail">停止並回報</option><option value="overwrite">覆寫</option></select></label>
    </div>
    <label>檔名<input aria-label="匯出檔名範本" value={settings.filenameTemplate} onChange={(event) => onChange({ ...settings, filenameTemplate: event.target.value })} /></label>
    <label><input type="checkbox" checked={settings.embedProfile} onChange={(event) => onChange({ ...settings, embedProfile: event.target.checked })} /> 內嵌 ICC 描述檔</label>
    <label><input type="checkbox" checked={settings.includeLocation} disabled={settings.metadata === 'none' || settings.metadata === 'copyrightOnly'} onChange={(event) => onChange({ ...settings, includeLocation: event.target.checked })} /> 包含 GPS 位置（預設關閉）</label>
    <div className="portrait-regions"><button disabled={!nativeAvailable || busy} onClick={onExport}>{busy ? '正在匯出…' : `匯出 ${selectedCount > 1 ? selectedCount : 1} 張`}</button>{busy && <button onClick={onCancel}>安全取消</button>}</div>
  </section>
}

function CommandPalette({ query, setQuery, execute, close }: {
  query: string; setQuery: (value: string) => void; execute: (id: CommandId) => void; close: () => void
}) {
  const matches = searchCommands(query)
  const dialogRef = useRef<HTMLElement>(null)
  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    return () => previousFocus?.focus()
  }, [])
  const keepFocusInDialog = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      close()
      return
    }
    if (event.key !== 'Tab') return
    const focusable = [...(dialogRef.current?.querySelectorAll<HTMLElement>('input, button:not(:disabled), [tabindex]:not([tabindex="-1"])') ?? [])]
    if (!focusable.length) return
    const first = focusable[0]
    const last = focusable.at(-1)!
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
  }
  return <div className="command-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close() }}>
    <section ref={dialogRef} className="command-palette" role="dialog" aria-modal="true" aria-label="命令選擇器" onKeyDown={keepFocusInDialog}>
      <label><span>命令選擇器</span><input autoFocus value={query} placeholder="搜尋命令…" aria-label="搜尋命令"
        onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => {
          if (event.key === 'Enter' && matches[0]) execute(matches[0].id)
        }} /></label>
      <div role="listbox" aria-label="可用命令">
        {matches.map((command) => <button key={command.id} role="option" aria-selected="false" onClick={() => execute(command.id)}>
          <span>{command.label}</span><kbd>{command.shortcut}</kbd>
        </button>)}
        {!matches.length && <p>找不到符合的命令</p>}
      </div>
    </section>
  </div>
}

function AppHeader({ view, setView, theme, setTheme, before, setBefore, canUndo, canRedo, undo, redo, onRetouch, onExport, exportBusy }: {
  view: WorkspaceView; setView: (view: WorkspaceView) => void
  theme: Theme; setTheme: (theme: Theme) => void
  before: boolean; setBefore: (value: boolean) => void; canUndo: boolean; canRedo: boolean
  undo: () => void; redo: () => void; onRetouch: () => void; onExport: () => void; exportBusy: boolean
}) {
  return <header className="topbar">
    <div className="brand"><span className="brand-mark"><Aperture size={18} /></span><strong>Starroom</strong></div>
    <nav aria-label="工作區">
      <button className={view === 'library' ? 'active' : ''} onClick={() => setView('library')}>圖庫</button>
      <button className={view === 'edit' ? 'active' : ''} onClick={() => setView('edit')}>編輯</button>
      <button onClick={onRetouch}>修飾</button>
      <button className={view === 'compare' ? 'active' : ''} onClick={() => setView('compare')}>比較</button>
    </nav>
    <div className="top-actions">
      <button className={before ? 'text-button active' : 'text-button'} onClick={() => setBefore(!before)}><Columns2 size={15} /> 編輯前</button>
      <IconButton label="復原" disabled={!canUndo} onClick={undo}><Undo2 size={17} /></IconButton>
      <IconButton label="重做" disabled={!canRedo} onClick={redo}><Redo2 size={17} /></IconButton>
      <select aria-label="主題" value={theme} onChange={(event) => setTheme(event.target.value as Theme)}><option value="dark">深色</option><option value="gray">灰色</option><option value="light">淺色</option></select>
      <button className="export-button" disabled={exportBusy} onClick={onExport}><Download size={15} /> {exportBusy ? '正在匯出…' : '匯出'}</button>
    </div>
  </header>
}

export function App() {
  const [theme, setTheme] = usePersistedValue<Theme>('starroom-theme', 'dark')
  const [leftOpen, setLeftOpen] = usePersistedValue('starroom-left-panel', true)
  const [filmstripOpen, setFilmstripOpen] = usePersistedValue('starroom-filmstrip', true)
  const [leftPanelWidth, setLeftPanelWidth] = usePersistedValue('starroom-left-panel-width', 224)
  const [rightPanelWidth, setRightPanelWidth] = usePersistedValue('starroom-right-panel-width', 420)
  const [photos, setPhotos] = useState<PhotoItem[]>([demoPhoto])
  const [selectedId, setSelectedId] = useState(demoPhoto.id)
  const [filter, setFilter] = useState<LibraryFilter>('all')
  const [developTab, setDevelopTab] = useState<'presets' | 'layers' | 'history'>('history')
  const [view, setView] = useState<WorkspaceView>('edit')
  const [tool, setTool] = useState<Tool>('light')
  const [selectedCurvePoint, setSelectedCurvePoint] = useState<string | null>('midtone')
  const [curveChannel, setCurveChannel] = useState<keyof NativeToneCurves>('master')
  const [before, setBefore] = useState(false)
  const [zoom, setZoom] = useState<'fit' | '100'>('fit')
  const [zoomScale, setZoomScale] = useState(1)
  const [displayScale, setDisplayScale] = useState(1)
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const panStart = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null)
  const [histogram, setHistogram] = useState(() => Array.from({ length: 48 }, () => 0))
  const [renderStatus, setRenderStatus] = useState('就緒')
  const [dimensions, setDimensions] = useState('—')
  const [notice, setNotice] = useState('')
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)
  const [dragActive, setDragActive] = useState(false)
  const [commandQuery, setCommandQuery] = useState('')
  const [copiedSettings, setCopiedSettings] = useState<EditSnapshot | null>(null)
  const [recoveryState, setRecoveryState] = useState<NativeSessionState | null>(null)
  const [sessionReady, setSessionReady] = useState(() => !nativeRuntimeAvailable())
  const pendingSession = useRef<NativeSessionState | null>(null)
  const currentSession = useRef<NativeSessionState | null>(null)
  const transientEditsPending = useRef(false)
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
  const [aiAvailability, setAiAvailability] = useState<NativeAiAvailability | null>(null)
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
  const [libraryCollections, setLibraryCollections] = useState<NativeLibraryCollection[]>([])
  const [selectedLibraryIds, setSelectedLibraryIds] = useState<number[]>([])
  const [librarySearch, setLibrarySearch] = useState('')
  const [libraryBusy, setLibraryBusy] = useState(false)
  const [libraryPage, setLibraryPage] = useState(0)
  const libraryAnchor = useRef<number | null>(null)
  const thumbnailEpoch = useRef(0)
  const [nativeHistory, setNativeHistory] = useState<NativeHistoryResult | null>(null)
  const [snapshotName, setSnapshotName] = useState('Version 1')
  const [snapshotCompareId, setSnapshotCompareId] = useState<string | null>(null)
  const openedHistoryAsset = useRef<number | null>(null)
  const pendingNativeBefore = useRef<NativeEditSettings | null>(null)
  const nativeHistoryTimer = useRef<number | null>(null)
  const applyingNativeHistory = useRef(false)
  const [exportSettings, setExportSettings] = useState<NativeProfessionalExportSettings>({ format: 'jpeg', bitDepth: 8, quality: 92, colorSpace: 'srgb', embedProfile: true,
    resize: { mode: 'original' }, outputSharpen: 'off', sharpenAmount: 'standard', metadata: 'allMetadata', includeLocation: false,
    copyright: null, filenameTemplate: '{original_name}-starroom', collision: 'autoRename' })
  const [exportBusy, setExportBusy] = useState(false)
  const [exportPanelOpen, setExportPanelOpen] = useState(false)
  const [previewInteraction, setPreviewInteraction] = useState<'interactive' | 'final'>('final')
  const fileInput = useRef<HTMLInputElement>(null)
  const objectUrls = useRef(new Set<string>())

  const resizePanel = (side: 'left' | 'right', event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const origin = event.clientX
    const initial = side === 'left' ? leftPanelWidth : rightPanelWidth
    const move = (next: PointerEvent) => {
      const delta = next.clientX - origin
      const value = Math.round(Math.max(side === 'left' ? 180 : 300, Math.min(side === 'left' ? 380 : 620, initial + (side === 'left' ? delta : -delta))))
      if (side === 'left') setLeftPanelWidth(value); else setRightPanelWidth(value)
    }
    const stop = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', stop); document.body.classList.remove('panel-resizing') }
    document.body.classList.add('panel-resizing')
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', stop, { once: true })
  }

  useEffect(() => {
    if (!nativeRuntimeAvailable()) return
    void queryNativeAiAvailability()
      .then(setAiAvailability)
      .catch(() => setAiAvailability({
        faceSkin: { state: 'error', detail: 'Availability check failed' },
        subjectBackground: { state: 'error', detail: 'Availability check failed' },
        sky: { state: 'error', detail: 'Availability check failed' },
        denoise: { state: 'error', detail: 'Availability check failed' },
      }))
  }, [])

  const restoreSession = useCallback((state: NativeSessionState) => {
    const workspaces: WorkspaceView[] = ['library', 'edit', 'compare']
    const tools = toolItems.map(({ id }) => id)
    const filters: LibraryFilter[] = ['all', 'recent', 'five-star', 'edited']
    if (workspaces.includes(state.workspace)) setView(state.workspace)
    if (tools.includes(state.activeTool as Tool)) setTool(state.activeTool as Tool)
    if (filters.includes(state.libraryContext as LibraryFilter)) setFilter(state.libraryContext as LibraryFilter)
    setLeftOpen(state.libraryPanelOpen)
    setFilmstripOpen(state.filmstripOpen)
    setZoom(state.zoomMode)
    setZoomScale(state.zoomScale)
    pendingSession.current = state
  }, [setFilmstripOpen, setLeftOpen])

  useEffect(() => () => { thumbnailEpoch.current++; objectUrls.current.forEach((url) => URL.revokeObjectURL(url)) }, [])
  const loadLibraryThumbnails = useCallback((assets: NativeLibraryAsset[]) => {
    const epoch = ++thumbnailEpoch.current
    void loadProgressiveThumbnails(assets.map((asset) => asset.id), nativeLibraryThumbnail,
      (id, src) => setPhotos((current) => current.map((photo) => photo.libraryAsset?.id === id ? { ...photo, src } : photo)),
      (_id, error) => setNotice(formatUserError(error, 'Thumbnail could not be loaded')),
      () => thumbnailEpoch.current === epoch)
  }, [])
  useEffect(() => {
    if (!nativeRuntimeAvailable()) return
    void openNativeSession().then((result) => {
      if (result.recoveryAvailable && result.state) setRecoveryState(result.state)
      else { if (result.state) restoreSession(result.state); setSessionReady(true) }
    }).catch((error) => { setNotice(formatUserError(error, 'Session restore failed')); setSessionReady(true) })
  }, [restoreSession])
  useEffect(() => {
    const finish = () => setPreviewInteraction('final')
    window.addEventListener('pointerup', finish)
    window.addEventListener('pointercancel', finish)
    window.addEventListener('focusout', finish)
    return () => {
      window.removeEventListener('pointerup', finish)
      window.removeEventListener('pointercancel', finish)
      window.removeEventListener('focusout', finish)
    }
  }, [])
  useEffect(() => {
    if (!nativeRuntimeAvailable()) return
    let active = true
    void (async () => {
      try {
        await openNativeLibrary()
        const assets = await queryNativeLibrary({ limit: 200 })
        const collections = await nativeLibraryCollections()
        if (!active) return
        setLibraryAssets(assets)
        setLibraryCollections(collections)
        const libraryPhotos = assets.map((asset) => libraryPhoto(asset, ''))
        if (libraryPhotos.length) {
          setPhotos((current) => [...libraryPhotos, ...current.filter((photo) => !photo.libraryAsset)])
          setSelectedLibraryIds([assets[0].id])
          loadLibraryThumbnails(assets)
        }
      } catch (error) {
        if (active) setNotice(formatUserError(error, 'Library initialization failed'))
      }
    })()
    return () => { active = false }
  }, [loadLibraryThumbnails])
  useEffect(() => {
    if (!notice) return
    const timeout = window.setTimeout(() => setNotice(''), 3500)
    return () => window.clearTimeout(timeout)
  }, [notice])

  const selectPhoto = useCallback((id: string) => {
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
    setSnapshotCompareId(null)
  }, [])

  const importNativePaths = useCallback((paths: readonly string[]) => {
    if (!paths.length) return
    setLibraryBusy(true)
    void (async () => {
      try {
        const result = await importNativeLibraryPaths(paths)
        const assets = await queryNativeLibrary({ limit: 200 })
        setLibraryAssets(assets)
        setLibraryPage(0)
        setLibrarySearch('')
        if (assets.length) setPhotos((current) => assets.map((asset) => {
          const existing = current.find((photo) => photo.libraryAsset?.id === asset.id)
          return existing ? { ...existing, name: asset.sourcePath.split(/[\\/]/).pop() ?? asset.sourcePath,
            sourcePath: asset.sourcePath, rating: asset.rating, libraryAsset: asset } : libraryPhoto(asset, '')
        }))
        loadLibraryThumbnails(assets)
        const selectedPath = paths[0]?.toLocaleLowerCase()
        const selectedAsset = assets.find((asset) => result.imported.includes(asset.id))
          ?? assets.find((asset) => asset.sourcePath.toLocaleLowerCase() === selectedPath) ?? assets[0]
        if (selectedAsset) {
          selectPhoto(`library-${selectedAsset.id}`)
          setSelectedLibraryIds([selectedAsset.id])
          setView('edit')
        }
        setFilter('all')
        setBefore(false)
        setNotice(`已加入 ${result.imported.length} 張照片 · ${result.unsupported.length} 個不支援 · ${result.failed.length} 個失敗${result.failed[0] ? `：${result.failed[0][1]}` : ''}`)
      } catch (error) { setNotice(formatUserError(error, 'Import failed')) }
      finally { setLibraryBusy(false) }
    })()
  }, [selectPhoto, loadLibraryThumbnails])

  const selected = photos.find((photo) => photo.id === selectedId) ?? photos[0]
  useEffect(() => { transientEditsPending.current = !selected.libraryAsset && hasPhotoEdits(selected) }, [selected])
  useEffect(() => {
    const pending = pendingSession.current
    if (!pending) return
    const photo = photos.find((candidate) => candidate.libraryAsset?.id === pending.selectedAssetId
      || (pending.selectedSourcePath && candidate.sourcePath === pending.selectedSourcePath))
    if (photo) { setSelectedId(photo.id); pendingSession.current = null }
  }, [photos])
  const sessionState = useMemo<NativeSessionState>(() => ({
    version: 1, workspace: view, selectedAssetId: selected.libraryAsset?.id ?? null,
    selectedSourcePath: selected.sourcePath ?? null, activeTool: tool,
    libraryPanelOpen: leftOpen, filmstripOpen, zoomMode: zoom, zoomScale,
    libraryContext: filter,
  }), [view, selected.libraryAsset?.id, selected.sourcePath, tool, leftOpen, filmstripOpen, zoom, zoomScale, filter])
  useEffect(() => { currentSession.current = sessionState }, [sessionState])
  useEffect(() => {
    if (!sessionReady || !nativeRuntimeAvailable()) return
    const timer = window.setTimeout(() => {
      void autosaveNativeSession(sessionState).catch((error) => setNotice(formatUserError(error, 'Autosave failed')))
    }, 500)
    return () => window.clearTimeout(timer)
  }, [sessionReady, sessionState])
  useEffect(() => {
    if (!nativeRuntimeAvailable()) return
    let unlisten: (() => void) | undefined
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const windowHandle = getCurrentWindow()
      unlisten = await windowHandle.onCloseRequested(async (event) => {
        event.preventDefault()
        if (transientEditsPending.current && !window.confirm('This Browser fallback photo has edits that are not stored in Native History. Close Starroom and discard those transient edits?')) return
        try {
          if (currentSession.current) await markNativeSessionClean(currentSession.current)
          await windowHandle.destroy()
        } catch (error) {
          // A failed clean-session write is recoverable. Keep the window alive and preserve the
          // interrupted autosave envelope instead of silently closing with ambiguous state.
          setNotice(formatUserError(error, 'Starroom could not save the session before closing'))
        }
      })
    }).catch(() => undefined)
    return () => unlisten?.()
  }, [])
  useEffect(() => {
    if (!nativeRuntimeAvailable()) return
    let unlisten: (() => void) | undefined
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      unlisten = await getCurrentWindow().onDragDropEvent(({ payload }) => {
        if (payload.type === 'enter' || payload.type === 'over') setDragActive(true)
        if (payload.type === 'leave') setDragActive(false)
        if (payload.type === 'drop') {
          setDragActive(false)
          importNativePaths(payload.paths)
        }
      })
    }).catch(() => undefined)
    return () => unlisten?.()
  }, [importNativePaths])
  const nativeHistoryState = useMemo(() => toNativeSettings(
    selected.adjustments, selected.curvePoints, selected.whiteBalanceMode, selected.whiteBalanceSample,
    selected.curveChannels, selected.opticsState, selected.layers, selected.mask,
    selected.skinRetouch, selected.healingOperations,
  ), [selected])
  const comparedSnapshot = nativeHistory?.snapshots.find((snapshot) => snapshot.id === snapshotCompareId) ?? null
  const snapshotComparePhoto = comparedSnapshot ? applyNativeHistoryState(selected, comparedSnapshot.state) : null

  useEffect(() => {
    const assetId = selected.libraryAsset?.id
    if (!assetId || openedHistoryAsset.current === assetId) return
    openedHistoryAsset.current = assetId
    void openNativeHistory(assetId, nativeHistoryState).then((result) => {
      applyingNativeHistory.current = true
      setNativeHistory(result)
      setPhotos((current) => current.map((photo) => photo.libraryAsset?.id === assetId ? applyNativeHistoryState(photo, result.state) : photo))
      window.setTimeout(() => { applyingNativeHistory.current = false }, 0)
    }).catch((error) => setNotice(formatUserError(error, 'History open failed')))
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
        .then(setNativeHistory).catch((error) => setNotice(formatUserError(error, 'History commit failed')))
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

  const counts = useMemo(() => {
    const nativePhotos = photos.filter((photo) => photo.libraryAsset)
    const visible = nativeRuntimeAvailable() ? nativePhotos : photos
    return {
      all: nativeRuntimeAvailable() ? libraryAssets.length : visible.length,
      recent: visible.filter((photo) => photo.imported).length,
      five: visible.filter((photo) => photo.rating === 5).length,
      edited: visible.filter(hasPhotoEdits).length,
    }
  }, [libraryAssets.length, photos])

  function nativeQuery(queryText: string, page: number, activeFilter: LibraryFilter): NativeLibraryQuery {
    return { text: queryText.trim() || null, limit: 200, offset: page * 200,
      sort: 'importTime', direction: 'descending', recentBatch: activeFilter === 'recent',
      minimumRating: activeFilter === 'five-star' ? 5 : null }
  }

  function chooseFilter(next: LibraryFilter) {
    setFilter(next)
    if (nativeRuntimeAvailable() && next !== 'edited') { void refreshLibrary(librarySearch, 0, next); return }
    const first = photos.find((photo) => next === 'all' || (next === 'recent' && photo.imported) || (next === 'five-star' && photo.rating === 5) || (next === 'edited' && hasPhotoEdits(photo)))
    if (first) selectPhoto(first.id)
  }

  function importPhotos(files: FileList | null) {
    if (!files?.length) return
    const supported = [...files].filter((file) => file.type.startsWith('image/'))
    if (!supported.length) {
      setNotice('未選取瀏覽器可讀取的影像。請使用 JPEG、PNG 或 WebP。')
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
    setNotice(`已匯入 ${imported.length} 張照片`)
  }

  async function refreshLibrary(queryText = librarySearch, page = libraryPage, activeFilter = filter) {
    if (!nativeRuntimeAvailable()) return
    setLibraryBusy(true)
    try {
      const assets = await queryNativeLibrary(nativeQuery(queryText, page, activeFilter))
      const existing = new Map(photos.filter((photo) => photo.libraryAsset).map((photo) => [photo.libraryAsset!.id, photo]))
      const rows = assets.map((asset) => {
        const current = existing.get(asset.id)
        if (current) return { ...current, name: asset.sourcePath.split(/[\\/]/).pop() ?? asset.sourcePath, sourcePath: asset.sourcePath, rating: asset.rating, libraryAsset: asset }
        return libraryPhoto(asset, '')
      })
      setLibraryAssets(assets)
      setLibraryPage(page)
      setPhotos((current) => [...rows, ...current.filter((photo) => !photo.libraryAsset)])
      loadLibraryThumbnails(assets.filter((asset) => !existing.get(asset.id)?.src))
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
      setNotice(`圖庫匯入 · 已加入 ${result.imported.length} 個 · ${result.duplicates.length} 個重複 · ${result.unsupported.length} 個不支援`)
    } catch (error) { setNotice(formatUserError(error, 'Library import failed')) }
    finally { setLibraryBusy(false) }
  }

  function selectLibraryAsset(event: MouseEvent, asset: NativeLibraryAsset) {
    const next = selectLibraryRange({ ids: selectedLibraryIds, anchor: libraryAnchor.current }, libraryAssets.map((value) => value.id), asset.id, event)
    libraryAnchor.current = next.anchor
    setSelectedLibraryIds(next.ids)
    setSelectedId(`library-${asset.id}`)
  }

  async function removeLibraryAssets(ids: number[]) {
    if (!ids.length || !window.confirm(`Remove ${ids.length} photo(s) from Library? Original files remain on disk.`)) return
    setLibraryBusy(true)
    try {
      const count = await removeNativeLibraryAssets(ids)
      setSelectedLibraryIds((current) => current.filter((id) => !ids.includes(id)))
      const remaining = photos.filter((photo) => !photo.libraryAsset || !ids.includes(photo.libraryAsset.id))
      setPhotos(remaining)
      setLibraryAssets((current) => current.filter((asset) => !ids.includes(asset.id)))
      if (!remaining.some((photo) => photo.id === selectedId) && remaining.length) selectPhoto(remaining[0].id)
      setNotice(`已從圖庫移除 ${count} 張照片 · 原始檔案未變更`)
    } catch (error) { setNotice(formatUserError(error, 'Library removal failed')) }
    finally { setLibraryBusy(false) }
  }

  async function updateLibraryWorkflow(values: { rating?: number; flag?: NativeAssetFlag; colorLabel?: NativeColorLabel }) {
    if (!selectedLibraryIds.length) return
    await updateNativeLibraryWorkflow(selectedLibraryIds, values)
    await refreshLibrary()
  }

  async function rateLibraryAsset(assetId: number, rating: number) {
    const nextRating = Math.max(0, Math.min(5, Math.trunc(rating)))
    setLibraryAssets((current) => current.map((asset) => asset.id === assetId ? { ...asset, rating: nextRating } : asset))
    setPhotos((current) => current.map((photo) => photo.libraryAsset?.id === assetId
      ? { ...photo, rating: nextRating, libraryAsset: { ...photo.libraryAsset, rating: nextRating } }
      : photo))
    try {
      await updateNativeLibraryWorkflow([assetId], { rating: nextRating })
      if (filter === 'five-star' && nextRating !== 5) await refreshLibrary(librarySearch, 0, filter)
    } catch (error) {
      setNotice(formatUserError(error, 'Rating update failed'))
      await refreshLibrary(librarySearch, libraryPage, filter)
    }
  }

  async function addLibraryKeyword(keyword: string) {
    if (!selectedLibraryIds.length) return
    await addNativeLibraryKeywords(selectedLibraryIds, [keyword])
    await refreshLibrary()
  }

  async function removeLibraryKeyword(keyword: string) {
    if (!selectedLibraryIds.length) return
    await removeNativeLibraryKeywords(selectedLibraryIds, [keyword])
    await refreshLibrary()
  }

  async function createLibraryCollection(kind: 'normal' | 'smart') {
    const name = window.prompt(kind === 'normal' ? 'Collection name' : 'Smart collection name')?.trim()
    if (!name) return
    let rule: { all: NativeSmartPredicate[] } | null = null
    if (kind === 'smart') {
      const rating = Math.max(0, Math.min(5, Number(window.prompt('Minimum rating (0-5)', '4')) || 0))
      const keyword = window.prompt('Required keyword (optional)', '')?.trim()
      rule = { all: [{ rating: { minimum: rating } }, ...(keyword ? [{ keyword: { value: keyword } } as NativeSmartPredicate] : [])] }
    }
    const id = await createNativeLibraryCollection(name, kind, rule)
    if (kind === 'normal' && selectedLibraryIds.length) await addNativeLibraryCollectionAssets(id, selectedLibraryIds)
    setLibraryCollections(await nativeLibraryCollections())
    setNotice(`已建立${kind === 'smart' ? '智慧型收藏集' : '收藏集'} · ${name}`)
  }

  async function openLibraryCollection(collection: NativeLibraryCollection) {
    const assets = await nativeLibraryCollectionAssets(collection.id, 200, 0)
    setLibraryAssets(assets)
    setPhotos((current) => [...assets.map((asset) => libraryPhoto(asset, '')), ...current.filter((photo) => !photo.libraryAsset)])
    loadLibraryThumbnails(assets)
    setSelectedLibraryIds(assets.length ? [assets[0].id] : [])
    setView('library')
    setNotice(`${collection.name} · ${assets.length} asset${assets.length === 1 ? '' : 's'}`)
  }

  async function requestPhotoImport() {
    if (!nativeRuntimeAvailable()) {
      fileInput.current?.click()
      return
    }
    try {
      const paths = await chooseNativePhotoPaths()
      if (!paths.length) return
      importNativePaths(paths)
    } catch (error) {
      setNotice(formatUserError(error, 'Native photo picker failed'))
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
    const nativeAsset = photos.find((photo) => photo.id === id)?.libraryAsset
    if (nativeAsset) { void removeLibraryAssets([nativeAsset.id]); return }
    if (photos.length <= 1) {
      setNotice('工作區中至少要保留一張照片')
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
    setNotice(`已從 Starroom 移除 ${removed?.name ?? '照片'} · 來源檔案未刪除`)
  }

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
    setPreviewInteraction('interactive')
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

  const nativeSettingsForPhoto = (photo: PhotoItem) => toNativeSettings(
    photo.adjustments, photo.curvePoints, photo.whiteBalanceMode,
    photo.whiteBalanceSample, photo.curveChannels, photo.opticsState,
    photo.layers, photo.mask, photo.skinRetouch, photo.healingOperations,
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
      setNotice(formatUserError(error, 'Reference match failed'))
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
      setNotice(formatUserError(error, 'Reference look save failed'))
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
      setNotice(formatUserError(error, 'Look save failed'))
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
      setNotice(formatUserError(error, 'Look load failed'))
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
      setNotice(formatUserError(error, 'Style mix failed'))
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
    setNotice('已儲存自訂曲線預設')
  }

  function loadCurvePreset() {
    if (!savedCurvePreset) return
    updateSelected((photo) => ({ ...photo, curvePoints: copyCurve(savedCurvePreset.master), curveChannels: copyCurveChannels(savedCurvePreset),
      history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
    setNotice('已載入自訂曲線預設')
  }

  function updateWhiteBalance(mode: NativeWhiteBalanceMode, sample: NativeWhiteBalanceSample | null = null) {
    updateSelected((photo) => ({ ...photo, whiteBalanceMode: mode, whiteBalanceSample: sample,
      history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
  }

  function copyWhiteBalance() {
    setCopiedWhiteBalance({ whiteBalanceMode: selected.whiteBalanceMode,
      whiteBalanceSample: selected.whiteBalanceSample ? { ...selected.whiteBalanceSample } : null })
    setNotice('已複製白平衡')
  }

  function pasteWhiteBalance() {
    if (!copiedWhiteBalance) { setNotice('請先複製白平衡'); return }
    updateWhiteBalance(copiedWhiteBalance.whiteBalanceMode,
      copiedWhiteBalance.whiteBalanceSample ? { ...copiedWhiteBalance.whiteBalanceSample } : null)
    setNotice('已貼上白平衡')
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
      setNotice(formatUserError(error, 'Native Color Mixer sampling failed'))
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
      setNotice(formatUserError(error, 'Lensfun resolution failed'))
    }
  }

  async function detectPortrait() {
    if (!selected.sourcePath || selected.renderBackend !== 'native') {
      setNotice('人像偵測需要原生照片；不會改用瀏覽器備援。')
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
      setNotice(formatUserError(error, 'Portrait detection failed'))
      setRenderStatus('Portrait detection failed')
    }
  }

  async function setupPortraitModels() {
    setRenderStatus('Verifying local YuNet + BiSeNet models…')
    try {
      const status = await installLocalPortraitModels()
      if (!status) { setRenderStatus('Portrait model setup cancelled'); return }
      setAiAvailability(status)
      setNotice('YuNet 與 BiSeNet 已驗證並安裝於本機；沒有上傳任何模型。')
      setRenderStatus('Local portrait models ready')
    } catch (error) {
      setNotice(formatUserError(error, 'Portrait model setup failed'))
      setRenderStatus('Portrait model verification failed')
      setAiAvailability(await queryNativeAiAvailability().catch(() => aiAvailability))
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
      setNotice(formatUserError(error, `${semantic} mask generation failed`))
      setRenderStatus('AI Mask unavailable')
    } finally {
      setAiMaskRequestId(null)
    }
  }

  async function cancelAiMask() {
    if (!aiMaskRequestId) return
    await cancelNativeAiMask(aiMaskRequestId)
    setNotice('已要求取消 AI 遮罩')
  }

  function updateSkinRetouch(mutator: (current: NativeSkinRetouchSettings) => NativeSkinRetouchSettings) {
    updateSelected((photo) => ({ ...photo, skinRetouch: copySkinRetouch(mutator(copySkinRetouch(photo.skinRetouch))), history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
    setBefore(false)
  }

  function enableSkinRetouch(faceId: string | '__all__') {
    if (!portraitDetection?.faces.length) {
      setNotice('啟用肌膚修飾前，請先在本機偵測人像')
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
      setNotice('本機建議需要原生影像；不會在未提示的情況下改用瀏覽器備援。')
      return
    }
    try {
      const result = await adviseNativeImage(selected.sourcePath, selected.adjustments, selected.curvePoints, selected.whiteBalanceMode, selected.whiteBalanceSample,
        selected.curveChannels, selected.opticsState, selected.layers, selected.skinRetouch, selected.healingOperations)
      setAdvisorResult(result)
      setNotice(`${result.suggestions.length} local, explainable suggestion${result.suggestions.length === 1 ? '' : 's'} ready`)
    } catch (error) { setNotice(formatUserError(error, 'Native advisor failed')) }
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
      void undoNativeHistory(selected.libraryAsset.id).then(applyHistoryResult).catch((error) => setNotice(formatUserError(error, 'Undo failed')))
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
      void redoNativeHistory(selected.libraryAsset.id).then(applyHistoryResult).catch((error) => setNotice(formatUserError(error, 'Redo failed')))
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
      .catch((error) => setNotice(formatUserError(error, 'Snapshot creation failed')))
  }

  function restoreSnapshot(snapshotId: string) {
    if (!selected.libraryAsset) return
    void restoreNativeSnapshot(selected.libraryAsset.id, snapshotId).then(applyHistoryResult)
      .catch((error) => setNotice(formatUserError(error, 'Snapshot restore failed')))
  }

  function renameSnapshot(snapshotId: string, currentName: string) {
    if (!selected.libraryAsset) return
    const name = window.prompt('Snapshot name', currentName)?.trim()
    if (!name) return
    void renameNativeSnapshot(selected.libraryAsset.id, snapshotId, name).then(setNativeHistory)
      .catch((error) => setNotice(formatUserError(error, 'Snapshot rename failed')))
  }

  function deleteSnapshot(snapshotId: string) {
    if (!selected.libraryAsset) return
    void deleteNativeSnapshot(selected.libraryAsset.id, snapshotId).then((result) => { setNativeHistory(result); if (snapshotCompareId === snapshotId) setSnapshotCompareId(null) })
      .catch((error) => setNotice(formatUserError(error, 'Snapshot delete failed')))
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

  function executeCommand(id: CommandId) {
    setCommandPaletteOpen(false)
    switch (id) {
      case 'undo': undo(); return
      case 'redo': redo(); return
      case 'copySettings':
        setCopiedSettings(takeSnapshot(selected))
        setNotice('已複製設定')
        return
      case 'pasteSettings':
        if (!copiedSettings) { setNotice('請先從照片複製設定'); return }
        updateSelected((photo) => ({ ...applySnapshot(photo, copiedSettings), history: [...photo.history, takeSnapshot(photo)].slice(-100), future: [] }))
        setBefore(false)
        setNotice('已透過共用編輯狀態貼上設定')
        return
      case 'before': setBefore((value) => !value); return
      case 'mask': setView('edit'); setTool('masks'); setBefore(false); return
      case 'healing': setView('edit'); setTool('heal'); setBefore(false); return
      case 'crop': setView('edit'); setTool('geometry'); setBefore(false); return
      case 'fit': setZoom('fit'); setZoomScale(1); setPan({ x: 0, y: 0 }); return
      case 'oneToOne': setZoom('100'); setZoomScale(1); setPan({ x: 0, y: 0 }); return
      case 'filmstrip': setFilmstripOpen((value) => !value); return
      case 'panels': setLeftOpen((value) => !value); return
      case 'export': void exportJpeg(); return
      case 'pick':
      case 'reject': {
        if (!selected.libraryAsset) { setNotice('Pick / Reject requires a Native Library asset'); return }
        void updateLibraryWorkflow({ flag: id === 'pick' ? 'pick' : 'reject' })
        return
      }
      default: {
        const rating = Number(id.slice(4))
        if (!Number.isInteger(rating) || rating < 0 || rating > 5) return
        updateSelected((photo) => ({ ...photo, rating }))
        if (selected.libraryAsset) void updateLibraryWorkflow({ rating })
      }
    }
  }

  async function exportJpeg() {
    setRenderStatus(selected.renderBackend === 'native' ? 'Native full-resolution export…' : 'Browser fallback export…')
    try {
      if (selected.renderBackend === 'native') {
        if (!selected.sourcePath) throw new Error('Native photo is missing its source path.')
        const destination = await chooseNativeExportDirectory()
        if (!destination) {
          setRenderStatus('Export cancelled')
          return
        }
        setExportBusy(true)
        const requestedPhotos = view === 'library' && selectedLibraryIds.length
          ? selectedLibraryIds.map((assetId) => photos.find((photo) => photo.libraryAsset?.id === assetId)).filter((photo): photo is PhotoItem => Boolean(photo))
          : [selected]
        if (requestedPhotos.some((photo) => photo.renderBackend !== 'native' || !photo.sourcePath)) throw new Error('Batch export requires Native source paths; Browser fallback is never silent.')
        const progressTimer = window.setInterval(() => {
          void queryNativeExportProgress().then(({ progress }) => {
            setRenderStatus(`Native export · ${progress.processed}/${progress.total} · ${progress.failed} failed`)
          }).catch(() => undefined)
        }, 250)
        const result = await (async () => {
          try {
            return await exportNativeBatch(destination, exportSettings, requestedPhotos.map((photo, index) => ({
              assetId: photo.libraryAsset?.id ?? 0, sourcePath: photo.sourcePath!, originalName: photo.name,
              captureDate: photo.libraryAsset?.metadata.captureTime ? new Date(photo.libraryAsset.metadata.captureTime * 1000).toISOString().slice(0, 10) : null,
              rating: photo.rating, keywords: photo.libraryAsset?.keywords ?? [], camera: photo.libraryAsset ? [photo.libraryAsset.metadata.cameraMake, photo.libraryAsset.metadata.cameraModel].filter(Boolean).join(' ') || null : null,
              look: null, sequence: index + 1, sourceFingerprint: photo.libraryAsset?.contentFingerprint ?? photo.sourcePath!,
              editStateIdentity: photo.id === selected.id ? nativeHistory?.stateVersion ?? 'workspace-transient' : `library-${photo.libraryAsset?.id ?? photo.id}`,
              editSettings: nativeSettingsForPhoto(photo),
            })))
          } finally {
            window.clearInterval(progressTimer)
          }
        })()
        setNotice(`專業匯出 · ${result.completed.length} 個完成 · ${result.failed.length} 個失敗`)
        setRenderStatus(`Native full-resolution · ${exportSettings.colorSpace}`)
        setExportBusy(false)
        return
      }
      throw new Error('Native desktop is required for editing and export. Browser creative rendering has been retired.')
    } catch (error) {
      setExportBusy(false)
      setNotice(formatUserError(error, 'Export failed'))
      setRenderStatus('Export failed')
    }
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const typing = target?.matches('input, select, textarea, [contenteditable="true"]')
      const modifier = event.ctrlKey || event.metaKey
      if (modifier && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setCommandQuery('')
        setCommandPaletteOpen(true)
        return
      }
      if (modifier && event.key.toLowerCase() === 'a' && view === 'library') {
        event.preventDefault()
        void queryNativeLibraryIds(nativeQuery(librarySearch, 0, filter)).then((ids) => {
          setSelectedLibraryIds(ids)
          libraryAnchor.current = ids[0] ?? null
          setNotice(`${ids.length} photos selected in current Library result`)
        }).catch((error) => setNotice(formatUserError(error, 'Select all failed')))
        return
      }
      if (event.key === 'Escape' && commandPaletteOpen) {
        event.preventDefault()
        setCommandPaletteOpen(false)
        return
      }
      if (typing || commandPaletteOpen) return
      const command = resolveCommandShortcut(event)
      if (command) {
        event.preventDefault()
        executeCommand(command)
      } else if (event.key === 'Delete') {
        event.preventDefault()
        if (view === 'library') void removeLibraryAssets(selectedLibraryIds)
        else removePhoto(selectedId)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  })

  return <main className={`app theme-${theme}`} data-theme={theme}>
    {dragActive && <div className="native-drop-overlay" role="status" aria-live="polite">將照片拖放到此處，以原生處理管線開啟</div>}
    {commandPaletteOpen && <CommandPalette query={commandQuery} setQuery={setCommandQuery} execute={executeCommand} close={() => setCommandPaletteOpen(false)} />}
    {recoveryState && <div className="command-backdrop" role="presentation"><section className="recovery-dialog" role="alertdialog" aria-modal="true" aria-labelledby="recovery-title">
      <span className="eyebrow">當機復原</span><h2 id="recovery-title">Starroom 發現中斷的工作階段</h2>
      <p>可還原先前的工作區、所選照片、面板與縮放狀態，或只捨棄復原資料。來源照片絕不會被修改。</p>
      <div><button className="export-button" autoFocus onClick={() => { restoreSession(recoveryState); setRecoveryState(null); setSessionReady(true) }}>還原</button>
        <button onClick={() => void discardNativeRecovery().then(() => { setRecoveryState(null); setSessionReady(true) }).catch((error) => setNotice(formatUserError(error, '無法捨棄復原資料')))}>捨棄</button></div>
    </section></div>}
    <AppHeader view={view} setView={(next) => { setView(next); setBefore(false) }} theme={theme} setTheme={setTheme} before={before} setBefore={setBefore}
      canUndo={selected.libraryAsset ? Boolean(nativeHistory?.canUndo) : selected.history.length > 0}
      canRedo={selected.libraryAsset ? Boolean(nativeHistory?.canRedo) : selected.future.length > 0} undo={undo} redo={redo}
      onRetouch={() => { setView('edit'); setTool('heal'); setBefore(false) }} onExport={() => { if (view === 'library') void exportJpeg(); else setExportPanelOpen(true) }} exportBusy={exportBusy} />
    <div className={`workspace view-${view} ${leftOpen ? '' : 'left-collapsed'} ${filmstripOpen ? '' : 'filmstrip-collapsed'}`}
      style={{ '--left-panel-width': `${leftPanelWidth}px`, '--right-panel-width': `${rightPanelWidth}px` } as CSSProperties}>
      <aside className="library-panel">
        <div className="panel-title"><span>{view === 'library' ? '圖庫' : '編輯'}</span><IconButton label="收合左側面板" onClick={() => setLeftOpen(false)}><PanelLeftClose size={17} /></IconButton></div>
        {view !== 'library' ? <div className="develop-left">
          <section className="navigator-card" aria-label="導覽器"><strong>導覽器</strong><img src={selected.src} alt="導覽預覽" /><small>{selected.name}</small></section>
          <div className="develop-tabs" role="tablist" aria-label="Develop sidebar">
            {(['presets', 'layers', 'history'] as const).map((tab) => <button key={tab} role="tab" aria-selected={developTab === tab} className={developTab === tab ? 'active' : ''} onClick={() => setDevelopTab(tab)}>{{ presets: '預設', layers: '圖層', history: '歷史記錄' }[tab]}</button>)}
          </div>
          {developTab === 'presets' && <section className="develop-tab-content"><strong>預設</strong><small>命名風格與曲線預設皆維持非破壞性編輯。</small></section>}
          {developTab === 'layers' && <section className="develop-tab-content"><strong>{selected.layers.length} 個圖層</strong>{selected.layers.map((layer) => <button key={layer.id} onClick={() => setSelectedLayerId(layer.id)}>{layer.name}</button>)}</section>}
          {developTab === 'history' && <section className="develop-tab-content history-panel" aria-label="Edit history and snapshots">
            <div className="layer-stack-head"><strong>歷史記錄／快照</strong><small>{nativeHistory?.stateVersion.slice(0, 8) ?? '開啟中'}</small></div>
            <div className="snapshot-create"><input aria-label="快照名稱" value={snapshotName} onChange={(event) => setSnapshotName(event.target.value)} /><button onClick={createSnapshot}>儲存</button></div>
            <div className="history-list">{nativeHistory?.snapshots.map((snapshot) => <div className="snapshot-row" key={snapshot.id}><button onClick={() => restoreSnapshot(snapshot.id)}><strong>{snapshot.name}</strong><small>還原</small></button><button onClick={() => { setSnapshotCompareId(snapshot.id); setView('compare') }}>比較</button><button onClick={() => renameSnapshot(snapshot.id, snapshot.name)}>重新命名</button><button onClick={() => deleteSnapshot(snapshot.id)}>刪除</button></div>)}</div>
            <div className="history-list">{nativeHistory?.entries.slice(-12).reverse().map((entry) => <div key={entry.sequence}><span>{entry.sequence}</span><strong>{entry.description}</strong><small>{entry.affectedStage}</small></div>)}</div>
          </section>}
        </div> : <>
        <button className="import-button" onClick={() => view === 'library' ? void importLibraryFolder() : void requestPhotoImport()}><ImagePlus size={16} /> {view === 'library' ? '匯入資料夾' : '加入照片'}</button>
        <input ref={fileInput} type="file" accept="image/jpeg,image/png,image/webp,image/svg+xml" multiple hidden onChange={(event) => { importPhotos(event.target.files); event.target.value = '' }} />
        <span className="format-note">Native: JPEG · PNG · TIFF · NEF · ARW · CR2/CR3 · DNG · RAF</span>
        <div className="library-group"><span className="eyebrow">工作區</span>
          <button className={`library-item ${filter === 'all' ? 'selected' : ''}`} onClick={() => chooseFilter('all')}><Grid2X2 size={16} /> 所有照片 <small>{counts.all}</small></button>
          <button className={`library-item ${filter === 'recent' ? 'selected' : ''}`} onClick={() => chooseFilter('recent')}><Folder size={16} /> 最近匯入 <small>{counts.recent}</small></button>
        </div>
        <div className="library-group"><span className="eyebrow">智慧型相簿</span>
          <button className={`library-item ${filter === 'five-star' ? 'selected' : ''}`} onClick={() => chooseFilter('five-star')}><Star size={16} /> 五星照片 <small>{counts.five}</small></button>
          <button className={`library-item ${filter === 'edited' ? 'selected' : ''}`} onClick={() => chooseFilter('edited')}><Contrast size={16} /> 已編輯 <small>{counts.edited}</small></button>
        </div>
        <div className="library-group"><span className="eyebrow">收藏集</span>
          {libraryCollections.map((collection) => <button className="library-item" key={collection.id} onClick={() => void openLibraryCollection(collection)}><Folder size={16} /> {collection.name}<small>{collection.kind}</small></button>)}
          <div className="portrait-regions"><button onClick={() => void createLibraryCollection('normal')}>+ 收藏集</button><button onClick={() => void createLibraryCollection('smart')}>+ 智慧型收藏集</button></div>
        </div>
        <div className="library-summary"><Library size={15} /><span>顯示 {libraryAssets.length} 張照片</span></div>
        </>}
      </aside>
      {leftOpen && <div className="panel-resizer panel-resizer-left" role="separator" aria-label="Resize left panel" aria-orientation="vertical" onPointerDown={(event) => resizePanel('left', event)} />}
      {!leftOpen && <button className="edge-toggle left" aria-label="開啟圖庫" onClick={() => setLeftOpen(true)}><PanelLeftOpen size={17} /></button>}

      {view === 'library' ? <section className="library-browser" aria-label="照片圖庫">
        <div className="library-browser-head"><div><span className="eyebrow">本機優先圖庫</span><h1>{libraryAssets.length} 個項目</h1></div>
          <div className="library-actions"><input aria-label="搜尋圖庫" value={librarySearch} placeholder="檔名、相機、鏡頭、關鍵字" onChange={(event) => setLibrarySearch(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') void refreshLibrary(librarySearch, 0) }} />
            <button disabled={libraryBusy} onClick={() => void refreshLibrary(librarySearch, 0)}>{libraryBusy ? '處理中…' : '搜尋'}</button>
            <button disabled={libraryBusy || libraryPage === 0} onClick={() => void refreshLibrary(librarySearch, libraryPage - 1)}>上一頁</button><button disabled={libraryBusy || libraryAssets.length < 200} onClick={() => void refreshLibrary(librarySearch, libraryPage + 1)}>下一頁</button>
            <button className="import-button compact" disabled={libraryBusy} onClick={() => void importLibraryFolder()}><ImagePlus size={16} /> 匯入資料夾</button></div></div>
        <button disabled={libraryBusy || !selectedLibraryIds.length} onClick={() => void removeLibraryAssets(selectedLibraryIds)}>從圖庫移除已選取的 {selectedLibraryIds.length} 個項目</button>
        <div className="photo-grid virtual-grid" role="grid" aria-rowcount={libraryAssets.length}>{libraryAssets.map((asset) => {
          const photo = photos.find((value) => value.libraryAsset?.id === asset.id)
          if (!photo) return null
          const selectedAsset = selectedLibraryIds.includes(asset.id)
          return <article key={asset.id} role="gridcell" className={selectedAsset ? 'photo-card selected' : 'photo-card'}>
            <button className="photo-card-preview" onClick={(event) => selectLibraryAsset(event, asset)} onDoubleClick={() => { selectPhoto(photo.id); setView('edit'); setBefore(false) }} title={`選取 ${photo.name}；雙擊進入編輯`}>
              {photo.src ? <img loading="lazy" src={photo.src} alt={photo.name} /> : <span className="missing-thumbnail">無法顯示縮圖</span>}
            </button>
            <div><span title={photo.name}>{photo.name}</span><small>{asset.missing ? '找不到來源' : asset.metadata.fileType.toUpperCase()} · {asset.keywords.join(', ') || '沒有關鍵字'}</small></div>
            <div className="thumbnail-rating" aria-label={`Rate ${photo.name}`}>
              {[1, 2, 3, 4, 5].map((rating) => <button key={rating} aria-label={`${rating} star${rating === 1 ? '' : 's'}`}
                className={asset.rating >= rating ? 'active' : ''} onClick={(event) => { event.stopPropagation(); void rateLibraryAsset(asset.id, asset.rating === rating ? 0 : rating) }}>
                <Star size={11} fill={asset.rating >= rating ? 'currentColor' : 'none'} />
              </button>)}
            </div>
          </article>
        })}</div>
      </section> : <section className="canvas-area">
          <div className="canvas-toolbar"><span>{selected.name}</span><div>
            <button className={selected.rating === 5 ? 'active rating-button' : 'rating-button'} onClick={toggleRating} title="切換五星評分"><Star size={12} fill={selected.rating === 5 ? 'currentColor' : 'none'} /> {selected.rating === 5 ? '5★' : '評分'}</button>
            <button className="remove-selected" disabled={photos.length <= 1} onClick={() => removePhoto(selected.id)} title="從 Starroom 移除，但不刪除來源檔案"><Trash2 size={12} /> 移除</button>
            <button className={zoom === 'fit' && zoomScale === 1 ? 'active' : ''} onClick={() => { setZoom('fit'); setZoomScale(1); setPan({ x: 0, y: 0 }) }}>適合視窗</button>
            <button className={zoom === '100' ? 'active' : ''} onClick={() => { setZoom('100'); setZoomScale(1); setPan({ x: 0, y: 0 }) }}>100%</button>
            <span className="zoom-percentage" aria-live="polite">{Math.round(displayScale * zoomScale * 100)}%</span>
          </div></div>
          {view === 'compare' ? <div className="compare-stage">
            <div className="compare-pane"><PreviewCanvas photo={snapshotComparePhoto ?? selected} before={!snapshotComparePhoto} zoom={zoom} metric={false} onHistogram={setHistogram} onStatus={setRenderStatus} onDimensions={setDimensions} /><span>{comparedSnapshot?.name ?? '原圖'}</span></div>
            <div className="compare-pane"><PreviewCanvas photo={selected} before={false} zoom={zoom} onHistogram={setHistogram} onStatus={setRenderStatus} onDimensions={setDimensions} /><span>編輯後</span></div>
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
              <PreviewCanvas key={selected.id} photo={selected} before={before} zoom={zoom} zoomScale={zoomScale} pan={pan} interactionPhase={previewInteraction} maskActive={tool === 'masks' && !before && !activeLayerIsBrush}
                onBeginMaskEdit={beginInteractiveEdit} onMaskChange={updateMask}
                healActive={tool === 'heal' && !before && selected.renderBackend === 'native'} onHealingStroke={addHealingStroke}
                brushActive={tool === 'masks' && !before && activeLayerIsBrush} onBrushStroke={addMaskBrushStroke}
                maskPreview={!before && maskOverlayVisible && activeLayer && 'type' in activeLayer.mask && activeLayer.mask.type === 'generated' ? activeLayer.mask : null}
                onWhiteBalancePick={(sample) => updateWhiteBalance('neutralPicker', sample)}
                onColorSample={tool === 'color' && mixerPicking ? pickMixerBand : undefined}
                onHistogram={setHistogram} onStatus={setRenderStatus} onDimensions={setDimensions} onDisplayScale={setDisplayScale} />
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
          <div className="canvas-footer"><span>{zoom === 'fit' && zoomScale === 1 ? 'Fit · ' : ''}{Math.round(displayScale * zoomScale * 100)}%</span><span className="status-dot" /><span>{renderStatus}</span><span>· {dimensions}</span>
            <span className="zoom-help"><Move size={12} /> Wheel to zoom · drag to pan</span>
            <button aria-label="Toggle filmstrip" onClick={() => setFilmstripOpen(!filmstripOpen)}>{filmstripOpen ? <PanelBottomClose size={16} /> : <PanelBottomOpen size={16} />}</button></div>
          <div className="filmstrip" aria-label="Filmstrip">
            {filteredPhotos.length ? filteredPhotos.map((photo) => <div key={photo.id} className="thumb-shell">
              <button className={photo.id === selected.id ? 'thumb active' : 'thumb'} onClick={() => { selectPhoto(photo.id); setBefore(false) }} title={photo.name}>
                <img src={photo.src} alt={photo.name} /><span>{photo.rating === 5 ? '★' : hasPhotoEdits(photo) ? 'E' : ''}</span>
              </button>
              <button className="thumb-delete" aria-label={`Remove ${photo.name}`} title="Remove from Starroom (source stays on disk)" disabled={photos.length <= 1} onClick={() => removePhoto(photo.id)}><Trash2 size={13} /></button>
            </div>) : <div className="empty-filmstrip">這個相簿中沒有符合的照片。</div>}
          </div>
        </section>}

      {view !== 'library' && <div className="panel-resizer panel-resizer-right" role="separator" aria-label="Resize right panel" aria-orientation="vertical" onPointerDown={(event) => resizePanel('right', event)} />}
      <aside className="inspector-panel">
        {exportPanelOpen && <div className="export-popover glass-popover"><button className="popover-close" aria-label="Close export settings" onClick={() => setExportPanelOpen(false)}>×</button>
          <ExportPanel settings={exportSettings} busy={exportBusy} selectedCount={view === 'library' ? Math.max(1, selectedLibraryIds.length) : 1}
            nativeAvailable={(view === 'library' && selectedLibraryIds.length ? selectedLibraryIds.map((id) => photos.find((photo) => photo.libraryAsset?.id === id)).filter(Boolean) : [selected]).every((photo) => photo?.renderBackend === 'native')}
            onChange={setExportSettings} onExport={() => void exportJpeg()} onCancel={() => void cancelNativeExport().then(() => setNotice('Export cancellation requested · completed files remain valid'))} />
        </div>}
        {view === 'library' && <LibraryMetadataPanel asset={libraryAssets.find((asset) => asset.id === selectedLibraryIds.at(-1)) ?? null}
          selectedCount={selectedLibraryIds.length} onWorkflow={(values) => void updateLibraryWorkflow(values)} onAddKeyword={(keyword) => void addLibraryKeyword(keyword)} onRemoveKeyword={(keyword) => void removeLibraryKeyword(keyword)} />}
        <div className="histogram-wrap"><Histogram values={histogram} /><div><span>LIVE</span><span>{dimensions}</span><span>CPU</span></div></div>
        {(tool === 'masks' || tool === 'heal') && <section className="portrait-panel" aria-label="人像遮罩">
          <div className="layer-stack-head"><strong>人像</strong><button onClick={detectPortrait} disabled={selected.renderBackend !== 'native' || aiAvailability?.faceSkin.state !== 'ready'}>偵測人臉</button></div>
          <div className={`ai-availability state-${aiAvailability?.faceSkin.state ?? 'checking'}`}><strong>人臉／肌膚 · {aiAvailability?.faceSkin.state === 'ready' ? '就緒' : aiAvailability?.faceSkin.state === 'modelNotInstalled' ? '尚未安裝模型' : aiAvailability?.faceSkin.state ?? '檢查中'}</strong><small>{aiAvailability?.faceSkin.detail ?? '正在檢查本機模型檔案…'}</small></div>
          {aiAvailability?.faceSkin.state !== 'ready' && <button className="import-button portrait-model-setup" onClick={() => void setupPortraitModels()} disabled={!nativeRuntimeAvailable()}>選擇本機 YuNet＋BiSeNet 模型</button>}
          {selected.renderBackend !== 'native' && <small>需要原生影像；不會在未提示的情況下改用瀏覽器備援。</small>}
          {portraitDetection && <div className={`portrait-status status-${portraitDetection.status}`}>
            <strong>{portraitDetection.status === 'ready' ? `${portraitDetection.faces.length} face(s)` : portraitDetection.status}</strong>
            <small>{portraitDetection.error?.message ?? `YuNet ${portraitDetection.detectorModelVersion.slice(0, 8)} · BiSeNet ResNet18 · ${portraitDetection.executionProvider === 'directMl' ? 'DirectML' : 'CPU'}`}</small>
          </div>}
          {portraitDetection?.faces.map(({ face, cacheKey }, index) => <div className={portraitFaceId === face.id ? 'portrait-face selected' : 'portrait-face'} key={face.id}>
            <button onClick={() => setPortraitFaceId(face.id)}>人臉 {index + 1} · {Math.round(face.confidence * 100)}%</button>
            {portraitFaceId === face.id && <div className="portrait-regions">{(['face', 'skin', 'eyes', 'brows', 'lips', 'hair'] as NativePortraitRegion[]).map((region) =>
              <button key={region} onClick={() => addPortraitMask(face.id, cacheKey, region)}>{region}</button>)}</div>}
          </div>)}
          {portraitDetection?.faces.length && <div className={portraitFaceId === '__all__' ? 'portrait-face selected' : 'portrait-face'}>
            <button onClick={() => setPortraitFaceId('__all__')}>所有人臉</button>
            {portraitFaceId === '__all__' && <div className="portrait-regions">{(['face', 'skin', 'eyes', 'brows', 'lips', 'hair'] as NativePortraitRegion[]).map((region) =>
              <button key={region} onClick={() => addAllPortraitMasks(region)}>{region}</button>)}</div>}
          </div>}
          <div className="skin-retouch-panel" aria-label="AI 遮罩">
            <div className="layer-stack-head"><strong>AI 遮罩</strong>{aiMaskRequestId && <button onClick={cancelAiMask}>取消</button>}</div>
            <small>僅使用本機 ONNX · 可編輯的遮罩節點 · 像素資料不會經過 IPC 傳輸</small>
            <div className="portrait-regions">
              {(['subject', 'background', 'sky'] as const).map((semantic) => { const availability = semantic === 'sky' ? aiAvailability?.sky : aiAvailability?.subjectBackground; return <button key={semantic} title={availability?.detail} disabled={selected.renderBackend !== 'native' || aiMaskRequestId !== null || availability?.state !== 'ready'} onClick={() => generateAiMask(semantic)}>{semantic}</button> })}
              <button disabled={!portraitDetection?.faces.length} onClick={() => addAllPortraitMasks('face')}>人物</button>
              <button disabled={!portraitDetection?.faces.length} onClick={() => addAllPortraitMasks('skin')}>肌膚</button>
              <button disabled={!portraitDetection?.faces.length} onClick={() => addAllPortraitMasks('hair')}>頭髮</button>
            </div>
            {aiMaskRequestId && <small>正在本機產生…仍可取消。</small>}
            {aiMaskResult && <small>{aiMaskResult.semanticClass} · {aiMaskResult.executionProvider === 'directMl' ? 'DirectML' : 'CPU fallback'} · {aiMaskResult.status}</small>}
            <label><input type="checkbox" checked={maskOverlayVisible} disabled={!activeLayer || !('type' in activeLayer.mask) || activeLayer.mask.type !== 'generated'} onChange={(event) => setMaskOverlayVisible(event.target.checked)} /> 顯示遮罩覆蓋</label>
          </div>
          <div className="skin-retouch-panel" aria-label="肌膚修飾">
            <div className="layer-stack-head"><strong>肌膚修飾</strong><button onClick={() => enableSkinRetouch(portraitFaceId === '__all__' ? '__all__' : portraitFaceId ?? '__all__')} disabled={!portraitDetection?.faces.length}>使用所選人臉</button></div>
            {selected.skinRetouch.faces.length === 0
              ? <small>請選擇本機偵測到的人臉。眼睛、眉毛、嘴唇與頭髮會自動受到保護。</small>
              : <small>{selected.skinRetouch.faces.length} cached face{selected.skinRetouch.faces.length === 1 ? '' : 's'} · Native shared graph</small>}
            <div className="mask-controls">
              {([
                ['柔膚', 'smooth', 0, 100, 1], ['保留紋理', 'texture', 0, 100, 1], ['膚色均勻', 'toneEvenness', 0, 100, 1],
                ['肌膚色相', 'hueDegrees', -30, 30, 1], ['肌膚彩度', 'chroma', -50, 50, 1], ['臉部曝光', 'exposureEv', -2, 2, .05],
              ] as const).map(([label, key, min, max, step]) => {
                const raw = selected.skinRetouch.parameters[key]
                const value = key === 'texture' || key === 'smooth' || key === 'toneEvenness' ? Math.round(raw * 100) : key === 'chroma' ? Math.round(raw * 100) : raw
                return <label key={key}>{label}<input aria-label={`Skin ${label}`} type="number" min={min} max={max} step={step} value={value}
                  disabled={selected.skinRetouch.faces.length === 0}
                  onChange={(event) => { const next = Number(event.target.value); if (!Number.isFinite(next)) return; updateSkinRetouch((current) => ({ ...current, parameters: { ...current.parameters, [key]: key === 'texture' || key === 'smooth' || key === 'toneEvenness' || key === 'chroma' ? next / 100 : next } })) }} /></label>
              })}
            </div>
          </div>
          <div className="skin-retouch-panel" aria-label="修復筆刷">
            <div className="layer-stack-head"><strong>修復筆刷</strong><button onClick={() => setTool('heal')} disabled={selected.renderBackend !== 'native'}>筆刷</button></div>
            <small>在原生預覽上拖曳即可建立不受縮放影響、帶羽化的修復筆觸。自動取樣來源具確定性；目前不提供 AI 填補。</small>
            {selected.healingOperations.length > 0 && (() => {
              const operation = selected.healingOperations.at(-1)!
              const patch = (changes: Partial<NativeHealingOperation>) => updateHealingOperations((current) => current.map((value, index) => index === current.length - 1 ? { ...value, ...changes } : value))
              return <div className="mask-controls">
                <label>模式<select aria-label="修復模式" value={operation.mode} onChange={(event) => patch({ mode: event.target.value as NativeHealingOperation['mode'] })}><option value="heal">修復</option><option value="clone">仿製</option></select></label>
                <label>來源<select aria-label="修復來源模式" value={operation.sourceMode} onChange={(event) => patch({ sourceMode: event.target.value as NativeHealingOperation['sourceMode'], source: event.target.value === 'manual' ? operation.source ?? { x: .5, y: .5 } : null })}><option value="auto">自動</option><option value="manual">手動</option></select></label>
                {([['Radius', 'radius', .5, 512, .5], ['Feather', 'feather', 0, 1, .01], ['Opacity', 'opacity', 0, 1, .01], ['Angle', 'rotationDegrees', -180, 180, 1], ['Scale', 'scale', .1, 4, .01]] as const).map(([label, key, min, max, step]) => <label key={key}>{label}<input aria-label={`Healing ${label}`} type="number" value={operation[key]} min={min} max={max} step={step} onChange={(event) => { const value = Number(event.target.value); if (Number.isFinite(value)) patch({ [key]: Math.max(min, Math.min(max, value)) }) }} /></label>)}
                {operation.sourceMode === 'manual' && <><label>來源 X<input aria-label="修復來源 X" type="number" min="0" max="1" step=".01" value={operation.source?.x ?? .5} onChange={(event) => patch({ source: { x: Math.max(0, Math.min(1, Number(event.target.value) || 0)), y: operation.source?.y ?? .5 } })} /></label><label>來源 Y<input aria-label="修復來源 Y" type="number" min="0" max="1" step=".01" value={operation.source?.y ?? .5} onChange={(event) => patch({ source: { x: operation.source?.x ?? .5, y: Math.max(0, Math.min(1, Number(event.target.value) || 0)) } })} /></label></>}
                <label><input type="checkbox" checked={operation.toneAdaptation} onChange={(event) => patch({ toneAdaptation: event.target.checked })} /> Tone adapt</label><label><input type="checkbox" checked={operation.textureAdaptation} onChange={(event) => patch({ textureAdaptation: event.target.checked })} /> Texture adapt</label>
                <button onClick={() => updateHealingOperations((current) => current.slice(0, -1))}>移除上一筆</button><small>{selected.healingOperations.length} 筆操作</small>
              </div>
            })()}
          </div>
          <div className="skin-retouch-panel" aria-label="本機編輯建議">
            <div className="layer-stack-head"><strong>本機建議</strong><button onClick={runAdvisor} disabled={selected.renderBackend !== 'native'}>分析</button></div>
            <small>使用具確定性的本機統計與明確規則，不使用雲端、GPT 或機器學習信心分數。</small>
            {advisorResult && <div className="advisor-results"><small>p01 {advisorResult.analysis.p01.toFixed(3)} · p50 {advisorResult.analysis.p50.toFixed(3)} · p99 {advisorResult.analysis.p99.toFixed(3)}</small>
              {advisorPreview && <div className="portrait-regions"><button onClick={acceptAdvisorPreview}>套用預覽</button><button onClick={cancelAdvisorPreview}>取消預覽</button></div>}
              <button disabled={!advisorResult.suggestions.length} onClick={() => { applyAdvisorSuggestions(advisorResult.suggestions); setAdvisorResult(null) }}>套用所有安全建議</button><button onClick={() => setAdvisorResult(null)}>關閉</button>
              {advisorResult.suggestions.map((suggestion) => <div key={suggestion.id} className="portrait-face"><strong>{suggestion.what}</strong><small>{suggestion.why} · {suggestion.confidence}</small><span>{suggestion.control} {suggestion.amount > 0 ? '+' : ''}{suggestion.amount.toFixed(suggestion.control === 'exposure' ? 2 : 0)}</span><button onClick={() => previewAdvisorSuggestion(suggestion)}>預覽</button><button onClick={() => { applyAdvisorSuggestions([suggestion]); setAdvisorResult((current) => current ? { ...current, suggestions: current.suggestions.filter((item) => item.id !== suggestion.id) } : current) }}>套用</button><button onClick={() => setAdvisorResult((current) => current ? { ...current, suggestions: current.suggestions.filter((item) => item.id !== suggestion.id) } : current)}>忽略</button></div>)}</div>}
          </div>
        </section>}
        {tool === 'detail' && <section className={`ai-availability detail-availability state-${aiAvailability?.denoise.state ?? 'checking'}`} aria-label="AI Denoise availability"><strong>AI Denoise · {aiAvailability?.denoise.state === 'ready' ? 'Ready' : aiAvailability?.denoise.state === 'modelNotInstalled' ? 'Model not installed' : aiAvailability?.denoise.state ?? 'Checking'}</strong><small>{aiAvailability?.denoise.detail ?? 'Checking local model file…'}</small></section>}
        {tool === 'masks' && <section className="layer-stack" aria-label="調整圖層">
          <div className="layer-stack-head"><strong>圖層</strong><button onClick={addLayer}>+ 新增</button></div>
          {selected.layers.length === 0 ? <small>尚無局部調整圖層</small> : selected.layers.map((layer, index) => <div className={selectedLayerId === layer.id ? 'layer-row selected' : 'layer-row'} key={layer.id} onClick={() => setSelectedLayerId(layer.id)}>
            <input aria-label={`Enable ${layer.name}`} type="checkbox" checked={layer.enabled} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, enabled: event.target.checked }))} />
            <input aria-label="Layer name" value={layer.name} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, name: event.target.value.slice(0, 80) || 'Adjustment layer' }))} />
            <label>遮罩 <select aria-label="圖層遮罩類型" value={'type' in layer.mask ? layer.mask.type : 'none'} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, mask: newMaskOfType(event.target.value as 'none' | 'radial' | 'linear' | 'brush' | 'luminance' | 'colorRange') }))}><option value="none">無</option><option value="radial">放射狀</option><option value="linear">線性</option><option value="brush">筆刷</option><option value="luminance">明度範圍</option><option value="colorRange">色彩範圍</option><option value="portraitSemantic" disabled>人像（請使用人像面板）</option></select></label>
            <label>不透明度 <input aria-label="圖層不透明度" type="number" min="0" max="100" value={Math.round(layer.opacity * 100)} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, opacity: Math.max(0, Math.min(1, Number(event.target.value) / 100 || 0)) }))} /></label>
            <button aria-label="Move layer up" disabled={index === 0} onClick={() => moveLayer(layer.id, -1)}>↑</button><button aria-label="Move layer down" disabled={index === selected.layers.length - 1} onClick={() => moveLayer(layer.id, 1)}>↓</button>
            <button aria-label="複製圖層" onClick={() => duplicateLayer(layer.id)}>複製</button><button aria-label="刪除圖層" onClick={() => deleteLayer(layer.id)}>×</button>
            {selectedLayerId === layer.id && <><label>Exposure <input aria-label="Layer exposure" type="number" min="-5" max="5" step=".05" value={layer.adjustments.tone.exposureEv} onChange={(event) => updateLayer(layer.id, (current) => ({ ...current, adjustments: { tone: { ...current.adjustments.tone, exposureEv: Math.max(-5, Math.min(5, Number(event.target.value) || 0)) } } }))} /></label>
              {'type' in layer.mask && <LayerMaskControls mask={layer.mask} onChange={(mask) => updateLayer(layer.id, (current) => ({ ...current, mask }))} />}
            </>}
          </div>)}
        </section>}
        {tool === 'looks' && <section className="layer-stack" aria-label="參考與風格工作流程">
          <div className="layer-stack-head"><strong>參考／風格</strong><small>原生處理</small></div>
          <small>感知參考分析與 .srlook 插值由 Rust 執行；React 不執行創意影像運算。</small>
          <div className="portrait-regions">
            <button disabled={selected.renderBackend !== 'native'} onClick={selectReference}>選擇參考照片…</button>
            <button disabled={selected.renderBackend !== 'native'} onClick={analyzeReference}>分析</button>
            <button disabled={!referenceResult} onClick={previewReference}>預覽</button>
            <button disabled={!referenceResult} onClick={applyReference}>套用</button>
            <button disabled={!referenceBase} onClick={resetReference}>重設</button>
            <button disabled={!referenceResult} onClick={saveReferenceAsLook}>將比對結果儲存為風格…</button>
            <button disabled={selected.renderBackend !== 'native'} onClick={saveLookWorkflow}>儲存 .srlook…</button>
            <button disabled={selected.renderBackend !== 'native'} onClick={loadLookWorkflow}>載入 .srlook…</button>
          </div>
          <small>{referencePath ? `Reference: ${referencePath.split(/[\\/]/).pop()}` : 'No reference selected'}</small>
          {(Object.entries(referenceControls) as Array<[keyof typeof referenceControls, number]>).map(([key, value]) =>
            <label key={key}>{key === 'protectSkin' ? 'Protect skin' : key[0].toUpperCase() + key.slice(1)}
              <input aria-label={`Reference ${key}`} type="number" min="0" max="100" step="1" value={value}
                onChange={(event) => { setReferenceControls((controls) => ({ ...controls, [key]: Math.max(0, Math.min(100, Number(event.target.value) || 0)) })); setReferenceResult(null) }} />%</label>)}
          <label>風格強度 <input aria-label="風格強度" type="number" min="0" max="100" step="1" value={lookAmount}
            onChange={(event) => setLookAmount(Math.max(0, Math.min(100, Number(event.target.value) || 0)))} />%</label>
          <div className="portrait-regions">
            <button onClick={() => selectMixerLook('a')}>風格 A…</button>
            <button onClick={() => selectMixerLook('b')}>風格 B…</button>
            <button disabled={!lookAPath || !lookBPath || lookAWeight + lookBWeight === 0} onClick={applyStyleMixer}>套用 A／B 混合</button>
          </div>
          <small>A: {lookAPath?.split(/[\\/]/).pop() ?? 'not selected'} · B: {lookBPath?.split(/[\\/]/).pop() ?? 'not selected'}</small>
          <label>風格 A 權重 <input aria-label="風格 A 權重" type="number" min="0" max="100" value={lookAWeight}
            onChange={(event) => setLookAWeight(Math.max(0, Math.min(100, Number(event.target.value) || 0)))} />%</label>
          <label>風格 B 權重 <input aria-label="風格 B 權重" type="number" min="0" max="100" value={lookBWeight}
            onChange={(event) => setLookBWeight(Math.max(0, Math.min(100, Number(event.target.value) || 0)))} />%</label>
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
        <button className="reset-all" disabled={!hasPhotoEdits(selected)} onClick={resetAll}><RotateCcw size={14} /> 重設所有編輯</button>
      </aside>
    </div>
    {notice && <button className="notice" onClick={() => setNotice('')} aria-label="關閉通知">{notice}</button>}
    <div className="compact-warning"><SunMedium size={18} /><span>請加寬 Starroom 視窗，以顯示完整編輯工作區。</span></div>
  </main>
}
