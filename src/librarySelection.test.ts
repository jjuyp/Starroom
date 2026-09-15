import { describe, expect, it } from 'vitest'
import { selectLibraryRange } from './librarySelection'

const click = { shiftKey: false, ctrlKey: false, metaKey: false }
describe('query-ordered Library selection', () => {
  it('replaces, toggles, extends and adds a range without changing its anchor', () => {
    const order = [10, 3, 7, 2, 9]
    let state = selectLibraryRange({ ids: [], anchor: null }, order, 3, click)
    expect(state).toEqual({ ids: [3], anchor: 3 })
    state = selectLibraryRange(state, order, 2, { ...click, shiftKey: true })
    expect(state).toEqual({ ids: [3, 7, 2], anchor: 3 })
    state = selectLibraryRange(state, order, 9, { ...click, ctrlKey: true })
    expect(state.ids).toEqual([3, 7, 2, 9])
    state = selectLibraryRange(state, order, 2, { ...click, metaKey: true })
    expect(state.ids).toEqual([3, 7, 9])
    state = selectLibraryRange(state, order, 10, { ...click, shiftKey: true, ctrlKey: true })
    expect(state.ids).toEqual([3, 7, 9, 10, 2])
    expect(state.anchor).toBe(2)
  })
  it('uses stable identity across sorting and resets an absent filter anchor', () => {
    const state = { ids: [4], anchor: 4 }
    expect(selectLibraryRange(state, [8, 4, 2], 8, { ...click, shiftKey: true }).ids).toEqual([8, 4])
    expect(selectLibraryRange(state, [8, 2], 2, { ...click, shiftKey: true })).toEqual({ ids: [2], anchor: 2 })
    expect(selectLibraryRange(state, [8, 2], 99, click)).toBe(state)
  })
})
