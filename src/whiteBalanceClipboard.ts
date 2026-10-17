import type { Adjustments } from './editorState'
import type { NativeWhiteBalanceMode, NativeWhiteBalanceSample, NativeCameraWhiteBalanceTarget } from './nativeRender'

interface WhiteBalancePhotoState {
  rawWhiteBalance?: NativeCameraWhiteBalanceTarget | null
  adjustments: Adjustments
  whiteBalanceMode: NativeWhiteBalanceMode
  whiteBalanceSample: NativeWhiteBalanceSample | null
}

/** Copy edit intent, never the source camera's neutral/profile or unrelated controls. */
export interface WhiteBalanceClipboard {
  rawWhiteBalance?: NativeCameraWhiteBalanceTarget | null
  temperature: number
  tint: number
  whiteBalanceMode: NativeWhiteBalanceMode
  whiteBalanceSample: NativeWhiteBalanceSample | null
}

export function canApplyWhiteBalanceMode(mode: NativeWhiteBalanceMode, source: 'rawMetadata' | 'renderedRelative' | undefined): boolean {
  if (mode === 'asShot' || mode === 'camera') return source === 'rawMetadata'
  if (mode === 'relative') return source === 'renderedRelative'
  return true
}

export function copyWhiteBalanceState(photo: WhiteBalancePhotoState): WhiteBalanceClipboard {
  return { rawWhiteBalance: photo.rawWhiteBalance ? { ...photo.rawWhiteBalance } : null,
    temperature: photo.adjustments.temperature, tint: photo.adjustments.tint,
    whiteBalanceMode: photo.whiteBalanceMode,
    whiteBalanceSample: photo.whiteBalanceSample ? { ...photo.whiteBalanceSample } : null }
}

export function pasteWhiteBalanceState(photo: WhiteBalancePhotoState, clipboard: WhiteBalanceClipboard): WhiteBalancePhotoState {
  return { rawWhiteBalance: clipboard.rawWhiteBalance ? { ...clipboard.rawWhiteBalance } : null,
    adjustments: { ...photo.adjustments, temperature: clipboard.temperature, tint: clipboard.tint },
    whiteBalanceMode: clipboard.whiteBalanceMode,
    whiteBalanceSample: clipboard.whiteBalanceSample ? { ...clipboard.whiteBalanceSample } : null }
}
