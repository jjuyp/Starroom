import { describe, expect, it } from 'vitest'
import { presentError } from './errorPresentation'

describe('typed production error presentation', () => {
  it.each(['InvalidCurvePreview: malformed Native sample result', 'CurvePreviewUnavailable: Native runtime is required',
    'Command native_curve_preview not found'])('gives explicit curve guidance for %s', (diagnostic) => {
    expect(presentError(diagnostic, '曲線顯示失敗')).toEqual({ category: 'Color', diagnostic,
      message: '無法顯示原生曲線，請重新開啟曲線面板，必要時復原最近的曲線調整。' })
  })
  it('classifies the actual LittleCMS cache failure without hiding diagnostic detail', () => {
    expect(presentError('LittleCMS transform cache is unavailable', '預覽失敗')).toMatchObject({ category: 'Color',
      diagnostic: 'LittleCMS transform cache is unavailable' })
  })
  it.each([
    'InvalidWhiteBalanceSample',
    'Preview failed: neutral-picker sample is missing or invalid',
    'InvalidWhitePoint',
    'chromatic adaptation requires a finite, positive, nonsingular measured white point',
  ])('gives actionable neutral-white guidance for %s', (diagnostic) => {
    expect(presentError(diagnostic, '預覽失敗')).toEqual({ category: 'Color', diagnostic,
      message: '無法從這個區域取得有效白平衡。請選擇有亮度的中性灰或白色區域。' })
  })
  it('clearly explains immutable-source export protection without hiding diagnostics', () => {
    const diagnostic = 'SourceOverwriteForbidden: C:/照片/原始.png'
    expect(presentError(diagnostic, 'Export failed')).toEqual({ category: 'Export',
      message: '匯出不能覆寫原始照片。請更換檔名或匯出資料夾。', diagnostic })
  })
  it.each([
    ['OutOfMemory: estimated 3200000000 bytes', 'Memory'],
    ['DatabaseOpenFailed: corrupt database', 'Library'],
    ['SourceMissing: C:/photo.nef', 'Missing'],
    ['SessionInvalid: unsupported version', 'Session'],
    ['ICC profile is invalid', 'Color'],
    ['DetectorModelMissing: yunet.onnx', 'Missing'],
    ['permission denied', 'Permission'],
  ] as const)('classifies %s', (diagnostic, category) => {
    expect(presentError(diagnostic, 'Operation failed')).toMatchObject({ category, diagnostic })
  })

  it('preserves diagnostics while using a safe fallback for unknown errors', () => {
    expect(presentError({ unexpected: true }, 'Preview failed')).toEqual({ category: 'Unknown', message: 'Preview failed', diagnostic: 'Preview failed' })
  })

  it.each([
    ['MaskSourceMismatch: foreign source', '這個 AI 遮罩屬於另一張照片，請在目前照片重新產生遮罩。'],
    ['PortraitSourceMismatch: face belongs to another source', '這個人像或肌膚選取屬於另一張照片，請重新偵測並選取人臉。'],
    ['PortraitRestoreMetadataMissing: legacy crop hash', '這個舊人像選取缺少還原資料，請重新偵測並選取人臉。'],
  ])('gives actionable AI restore guidance for %s without hiding diagnostics', (diagnostic, message) => {
    expect(presentError(diagnostic, '預覽失敗')).toEqual({ category: 'AI', diagnostic, message })
  })
})
