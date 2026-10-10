import { describe, expect, it } from 'vitest'
import { withWindowsPathAlias } from '../windows-path-alias.mjs'

describe('owned Windows build alias', () => {
  it('does not remove an occupied or concurrently claimed letter', () => {
    const removed: string[] = []
    const result = withWindowsPathAlias('workspace', (drive: string) => {
      if (drive === 'R:') throw new Error('occupied')
    }, (drive: string) => removed.push(drive), (path: string) => {
      expect(path).toBe('S:\\'); return 7
    })
    expect(result).toBe(7); expect(removed).toEqual(['S:'])
  })
  it('cleans only its successfully acquired alias when the build throws', () => {
    const removed: string[] = []
    expect(() => withWindowsPathAlias('workspace', () => {}, (drive: string) => removed.push(drive),
      () => { throw new Error('build failure') })).toThrow('build failure')
    expect(removed).toEqual(['R:'])
  })
  it('never cleans another mapping when every acquisition fails', () => {
    const removed: string[] = []
    expect(() => withWindowsPathAlias('workspace', () => { throw new Error('occupied') },
      (drive: string) => removed.push(drive), () => 0)).toThrow('could be acquired')
    expect(removed).toEqual([])
  })
  it('nested concurrent leases retain separate ownership until each exits', () => {
    const occupied = new Set<string>(); const removed: string[] = []
    const map = (drive: string) => { if (occupied.has(drive)) throw new Error('occupied'); occupied.add(drive) }
    const unmap = (drive: string) => { removed.push(drive); occupied.delete(drive) }
    withWindowsPathAlias('workspace', map, unmap, (outer: string) => {
      expect(outer).toBe('R:\\')
      withWindowsPathAlias('workspace', map, unmap, (inner: string) => {
        expect(inner).toBe('S:\\'); expect(occupied.has('R:')).toBe(true); return 0
      })
      expect(occupied.has('R:')).toBe(true); return 0
    })
    expect(removed).toEqual(['S:', 'R:']); expect(occupied.size).toBe(0)
  })
})
