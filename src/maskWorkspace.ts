import type { NativeMaskDefinition, NativeMaskTree, NativeAdjustmentLayer, NativeSkinRetouchSettings } from './nativeRender'
import type { RadialMask } from './previewPresentation'

export const manualMaskTypes = ['radial', 'linear', 'brush', 'luminance', 'colorRange'] as const
export type ManualMaskType = typeof manualMaskTypes[number]
// Scene availability is shared by Sky and whole Person; Face/Skin keep their own provider.
export const generatedMaskActions = [
  { semantic: 'subject', label: '主體', availability: 'subjectBackground' },
  { semantic: 'background', label: '背景', availability: 'subjectBackground' },
  { semantic: 'person', label: '人物', availability: 'sky' },
  { semantic: 'sky', label: '天空', availability: 'sky' },
] as const
export const maskLabels: Record<NativeMaskDefinition['type'], string> = {
  none: '全圖', radial: '放射漸層', linear: '線性漸層', brush: '筆刷', luminance: '明度範圍',
  colorRange: '色彩範圍', portraitSemantic: '人像區域', generated: 'AI 選取',
}
export const portraitRegionLabels = { face: '臉部', skin: '肌膚', eyes: '眼睛', leftEye: '左眼', rightEye: '右眼', brows: '眉毛', leftBrow: '左眉', rightBrow: '右眉', lips: '嘴唇', mouth: '口腔', hair: '頭髮' } as const

export function validLinearGeometry(mask: Extract<NativeMaskDefinition, { type: 'linear' }>): boolean {
  return [mask.startX, mask.startY, mask.endX, mask.endY].every((value) => Number.isFinite(value) && value >= 0 && value <= 1)
    && Math.hypot(mask.endX - mask.startX, mask.endY - mask.startY) > .001
}

export function maskLabel(mask: NativeMaskTree): string {
  if ('type' in mask) return maskLabels[mask.type]
  return { add: '合併區域', subtract: '減去區域', intersect: '交集區域', invert: '反轉區域' }[mask.operation]
}

/** Native preserve-skin needs a real skin raster, not just installed face weights. */
export function hasNativeSkinSelection(skin: NativeSkinRetouchSettings, layers: NativeAdjustmentLayer[]): boolean {
  const hasSkin = (mask: NativeMaskTree): boolean => 'type' in mask
    ? mask.type === 'portraitSemantic' && mask.region === 'skin'
    : mask.children.some(hasSkin)
  return skin.faces.length > 0
    || layers.some((layer) => layer.enabled && hasSkin(layer.mask))
}

// Canvas controls edit only the selected layer. The legacy center mask has a separate contract.
export function selectedRadialMask(layer: NativeAdjustmentLayer | undefined): (RadialMask & { feather: number }) | null {
  return layer && 'type' in layer.mask && layer.mask.type === 'radial' ? layer.mask : null
}

export function replaceRadialGeometry(layer: NativeAdjustmentLayer, geometry: RadialMask): NativeAdjustmentLayer {
  if (!('type' in layer.mask) || layer.mask.type !== 'radial') return layer
  return { ...layer, mask: { ...layer.mask, ...geometry } }
}

export function invertedMask(mask: NativeMaskTree): NativeMaskTree {
  if ('type' in mask && 'invert' in mask) return { ...mask, invert: !mask.invert }
  if ('operation' in mask && mask.operation === 'invert' && mask.children.length === 1) return mask.children[0]
  return { operation: 'invert', children: [structuredClone(mask)] }
}
