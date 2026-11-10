import { describe, expect, it } from 'vitest'
import { CatalogEditMembership } from './catalogEditMembership'

describe('Native edited-count invalidation', () => {
  it('does not rescan history for every slider commit or snapshot of an already edited photo', () => {
    const membership = new CatalogEditMembership()
    expect(membership.observe(1, false)).toBe(false)
    expect(membership.observe(1, true)).toBe(true)
    for (let commit = 0; commit < 10000; commit++) expect(membership.observe(1, true)).toBe(false)
  })
  it('recounts undo-to-neutral, redo, restored snapshots and outgoing/off-page assets independently', () => {
    const membership = new CatalogEditMembership()
    expect(membership.observe(1, true)).toBe(true)
    expect(membership.observe(2, false)).toBe(false)
    expect(membership.observe(1, false)).toBe(true)
    expect(membership.observe(1, true)).toBe(true)
    expect(membership.observe(2, true)).toBe(true)
    expect(membership.observe(1, true)).toBe(false)
  })
  it('never treats unknown or corrupt membership as an unedited photo', () => {
    const membership = new CatalogEditMembership()
    expect(membership.observe(1, true)).toBe(true)
    expect(membership.observe(1, null)).toBe(true)
    expect(membership.observe(1, undefined)).toBe(true)
    expect(membership.observe(1, true)).toBe(false)
  })
})
