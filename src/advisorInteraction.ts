import type { AdjustmentKey, Adjustments } from './editorState'

/** Apply compact Native advice to an immutable UI baseline, never to the previous preview. */
export function advisorAdjustments(base: Adjustments, suggestions: ReadonlyArray<{ control: string; amount: number }>): Adjustments {
  const allowed = new Set<AdjustmentKey>(['exposure', 'shadows', 'highlights', 'contrast', 'temperature', 'tint'])
  const result = { ...base }
  for (const suggestion of suggestions) {
    if (!allowed.has(suggestion.control as AdjustmentKey) || !Number.isFinite(suggestion.amount)) continue
    const key = suggestion.control as AdjustmentKey
    const limit = key === 'exposure' ? 5 : 100
    result[key] = Math.max(-limit, Math.min(limit, result[key] + suggestion.amount))
  }
  return result
}
