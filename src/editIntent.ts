import { defaultNativeRenderConstants, type NativeRenderConstants, type NativeToneCurves,
  type NativeWhiteBalanceMode, type NativeWhiteBalanceSample } from './nativeRender'

export interface AdditionalEditIntent {
  curveChannels: NativeToneCurves
  whiteBalanceMode: NativeWhiteBalanceMode
  whiteBalanceSample: NativeWhiteBalanceSample | null
  nativeRenderConstants?: NativeRenderConstants
}

/** Counts declared edit intent, not image differences. Master/numeric edits are counted
 * by the existing workspace predicate; native RGB/WB/hidden controls must not be omitted. */
export function countAdditionalEditIntent(photo: AdditionalEditIntent): number {
  const channels = (['red', 'green', 'blue'] as const).filter((channel) => photo.curveChannels[channel].length > 0).length
  const constants = photo.nativeRenderConstants ?? defaultNativeRenderConstants
  const changedConstants = (Object.keys(defaultNativeRenderConstants) as Array<keyof NativeRenderConstants>)
    .some((key) => constants[key] !== defaultNativeRenderConstants[key])
  return channels + (photo.whiteBalanceMode !== 'sourceDefault' ? 1 : 0)
    + (photo.whiteBalanceSample ? 1 : 0) + (changedConstants ? 1 : 0)
}
