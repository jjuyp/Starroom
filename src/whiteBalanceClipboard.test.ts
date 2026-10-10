import { describe, expect, it } from 'vitest'
import { defaultAdjustments } from './editorState'
import { canApplyWhiteBalanceMode, copyWhiteBalanceState, pasteWhiteBalanceState } from './whiteBalanceClipboard'

describe('real white balance clipboard intent', () => {
  it('copies and pastes Temperature/Tint together with mode while preserving other edits', () => {
    const source = { adjustments: { ...defaultAdjustments, temperature: 28, tint: -14 },
      whiteBalanceMode: 'sourceDefault' as const, whiteBalanceSample: null }
    const target = { adjustments: { ...defaultAdjustments, exposure: 1.2, shadows: 42 },
      whiteBalanceMode: 'auto' as const, whiteBalanceSample: null }
    const result = pasteWhiteBalanceState(target, copyWhiteBalanceState(source))
    expect(result.adjustments).toEqual({ ...target.adjustments, temperature: 28, tint: -14 })
    expect(result.whiteBalanceMode).toBe('sourceDefault')
    expect(target.adjustments.temperature).toBe(0)
  })
  it('clones neutral sampling coordinates and never imports source camera calibration', () => {
    const source = { adjustments: { ...defaultAdjustments }, whiteBalanceMode: 'neutralPicker' as const,
      whiteBalanceSample: { x: .2, y: .3, width: .015, height: .015 } }
    const copied = copyWhiteBalanceState(source)
    source.whiteBalanceSample.x = .7
    const result = pasteWhiteBalanceState(source, copied)
    expect(result.whiteBalanceSample?.x).toBe(.2)
    expect(result.whiteBalanceSample).not.toBe(copied.whiteBalanceSample)
    expect(Object.keys(copied).sort()).toEqual(['temperature', 'tint', 'whiteBalanceMode', 'whiteBalanceSample'].sort())
  })
  it('refuses incompatible RAW/encoded mode pastes before making a broken render state', () => {
    expect(canApplyWhiteBalanceMode('camera', 'renderedRelative')).toBe(false)
    expect(canApplyWhiteBalanceMode('asShot', 'rawMetadata')).toBe(true)
    expect(canApplyWhiteBalanceMode('relative', 'rawMetadata')).toBe(false)
    expect(canApplyWhiteBalanceMode('relative', 'renderedRelative')).toBe(true)
    expect(canApplyWhiteBalanceMode('camera', undefined)).toBe(false)
    expect(canApplyWhiteBalanceMode('sourceDefault', 'rawMetadata')).toBe(true)
  })
})
