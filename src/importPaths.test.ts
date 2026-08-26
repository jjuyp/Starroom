import { describe, expect, it } from 'vitest'
import { supportedNativePhotoPaths } from './importPaths'

describe('native desktop file drops', () => {
  it('accepts every advertised encoded and RAW format case-insensitively', () => {
    const paths = ['a.JPG', 'b.png', 'c.TIFF', 'd.NEF', 'e.arw', 'f.CR2', 'g.cr3', 'h.DNG', 'i.raf']
    expect(supportedNativePhotoPaths(paths)).toEqual(paths)
  })

  it('rejects unsupported paths instead of creating a silent fallback', () => {
    expect(supportedNativePhotoPaths(['notes.txt', 'photo.webp', 'no-extension'])).toEqual([])
  })
})
