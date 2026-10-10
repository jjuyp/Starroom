import { expect, it } from 'vitest'
import { advisorAdjustments } from './advisorInteraction'
import { defaultAdjustments } from './editorState'

it('preview and apply from the same baseline without accumulating or mutating original state', () => {
  const base = { ...defaultAdjustments, exposure: .5 }
  const advice = [{ control: 'exposure', amount: .75 }]
  const preview = advisorAdjustments(base, advice)
  expect(preview.exposure).toBe(1.25)
  expect(advisorAdjustments(base, advice)).toEqual(preview)
  expect(base.exposure).toBe(.5)
  expect(advisorAdjustments(base, [{ control: 'shadows', amount: 20 }]).exposure).toBe(.5)
})

it('bounds native parameter suggestions and rejects unsupported or nonfinite advice', () => {
  expect(advisorAdjustments(defaultAdjustments, [{ control: 'exposure', amount: 99 }, { control: 'shadows', amount: -500 }])).toMatchObject({ exposure: 5, shadows: -100 })
  expect(advisorAdjustments(defaultAdjustments, [{ control: 'exposure', amount: NaN }, { control: 'rotation', amount: 90 }])).toEqual(defaultAdjustments)
})
