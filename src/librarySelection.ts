export interface LibrarySelection { ids: number[]; anchor: number | null }

/** The supplied order is the displayed query order, never database insertion order. */
export function selectLibraryRange(
  state: LibrarySelection, orderedIds: readonly number[], id: number,
  modifiers: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean },
): LibrarySelection {
  const index = orderedIds.indexOf(id)
  if (index < 0) return state
  const additive = modifiers.ctrlKey || modifiers.metaKey
  const anchorIndex = state.anchor === null ? -1 : orderedIds.indexOf(state.anchor)
  if (modifiers.shiftKey && anchorIndex >= 0) {
    const range = orderedIds.slice(Math.min(index, anchorIndex), Math.max(index, anchorIndex) + 1)
    return { anchor: state.anchor, ids: additive ? [...new Set([...state.ids, ...range])] : [...range] }
  }
  return { anchor: id, ids: additive
    ? state.ids.includes(id) ? state.ids.filter((value) => value !== id) : [...state.ids, id]
    : [id] }
}
