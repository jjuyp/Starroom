import { expect, it } from 'vitest'
import { InteractivePreviewBudget } from './interactivePreviewBudget'

it('responds to slow viewport-limited or source-limited frames, without treating them as fast full-tier samples', () => {
  const budget = new InteractivePreviewBudget()
  for (let i = 0; i < 8; i++) budget.observe(1024, 9, true)
  expect(budget.edge(3000, 6000)).toBe(2048)
  const viewportEdge = budget.edge(900, 6000)
  budget.observe(viewportEdge, 150, true)
  const smallSource = budget.edge(900, 700)
  budget.observe(smallSource, 150, true)
  expect(budget.edge(900, 700)).toBe(700) // different sizes do not accumulate misses
  budget.observe(smallSource, 150, true)
  expect(budget.edge(900, 700)).toBe(512)
})

it('bounds startup by viewport and source without full-resolution interaction', () => {
  const budget = new InteractivePreviewBudget()
  expect(budget.edge(4000, 10000)).toBe(1024)
  expect(budget.edge(400, 10000)).toBe(512)
  expect(budget.edge(4000, 700)).toBe(700)
  expect(budget.edge(NaN, NaN)).toBe(1024)
})

it('requires sustained misses and headroom, rejects old-tier observations', () => {
  const budget = new InteractivePreviewBudget()
  budget.observe(1024, 5000, true)
  expect(budget.edge(4000, 6000)).toBe(1024)
  budget.observe(1024, 90, true)
  expect(budget.edge(4000, 6000)).toBe(512)
  for (let i = 0; i < 20; i++) budget.observe(1024, 1, true)
  expect(budget.edge(4000, 6000)).toBe(512)
  for (let i = 0; i < 8; i++) budget.observe(512, 10, true)
  expect(budget.edge(4000, 6000)).toBe(1024)
  for (let i = 0; i < 8; i++) budget.observe(1024, 10, true)
  expect(budget.edge(4000, 6000)).toBe(2048)
  expect(budget.edge(800, 6000)).toBe(1024)
  budget.observe(2048, 30, false)
  expect(budget.edge(4000, 6000)).toBe(1024)
})

it('does not oscillate near budget or learn from invalid and small-image samples', () => {
  const budget = new InteractivePreviewBudget()
  for (const time of [NaN, Infinity, 0, -1, 51, 49, 51, 49]) budget.observe(1024, time, true)
  for (let i = 0; i < 20; i++) budget.observe(512, 1, true)
  expect(budget.edge(4000, 6000)).toBe(1024)
})

it('downshifts after consecutive 50–80 ms misses without oscillating on one miss', () => {
  const budget = new InteractivePreviewBudget()
  budget.observe(1024, 63, true)
  expect(budget.edge(1600, 6032)).toBe(1024)
  budget.observe(1024, 67, true)
  expect(budget.edge(1600, 6032)).toBe(512)
})
