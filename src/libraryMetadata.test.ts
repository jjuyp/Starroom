import { describe, expect, it } from 'vitest'
import { needsRawMetadataRepair } from './libraryMetadata'

describe('legacy RAW catalog metadata', () => {
  it('repairs missing or incorrectly persisted metadata-preview dimensions', () => {
    expect(needsRawMetadataRepair({ fileType: 'NEF', width: 32, height: 21 })).toBe(true)
    expect(needsRawMetadataRepair({ fileType: 'raf', width: null, height: null })).toBe(true)
  })
  it('does not decode correct RAW records or legitimately small encoded photos', () => {
    expect(needsRawMetadataRepair({ fileType: 'nef', width: 2012, height: 1324 })).toBe(false)
    expect(needsRawMetadataRepair({ fileType: 'png', width: 32, height: 21 })).toBe(false)
    expect(needsRawMetadataRepair({ fileType: 'jpg', width: null, height: null })).toBe(false)
  })
})
