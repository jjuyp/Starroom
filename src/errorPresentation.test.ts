import { describe, expect, it } from 'vitest'
import { presentError } from './errorPresentation'

describe('typed production error presentation', () => {
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
