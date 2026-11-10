import { describe, expect, it } from 'vitest'
import { appendWithinCapacity, EditorRequestGate } from './editorRequestGate'

describe('Native editor result lifetime', () => {
  it('discards results from an old selection, even after returning to that photo', () => {
    const gate = new EditorRequestGate(), old = gate.begin('portrait')
    gate.invalidate(); gate.invalidate()
    expect(old()).toBe(false)
    expect(gate.begin('portrait')()).toBe(true)
  })
  it('keeps independent tools valid but only the latest result within one tool', () => {
    const gate = new EditorRequestGate(), first = gate.begin('advisor'), portrait = gate.begin('portrait')
    const latest = gate.begin('advisor')
    expect(first()).toBe(false); expect(portrait()).toBe(true); expect(latest()).toBe(true)
  })
  it('rejects an over-capacity stroke in full, preserving old strokes without truncation', () => {
    const existing = [{ id: 'old' }], incoming = [{ id: 'a' }, { id: 'b' }]
    expect(appendWithinCapacity(existing, incoming, 2)).toEqual({ ok: false, remaining: 1 })
    expect(existing).toEqual([{ id: 'old' }])
    expect(incoming).toHaveLength(2)
  })
  it('accepts an exact-capacity append and retains operation order', () => {
    expect(appendWithinCapacity([1], [2, 3], 3)).toEqual({ ok: true, values: [1, 2, 3] })
  })
})
