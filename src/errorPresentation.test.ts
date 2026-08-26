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
})
