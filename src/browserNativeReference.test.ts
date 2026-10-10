import { describe, expect, it } from 'vitest'
import reference from '../tests/fixtures/m1c/browser-native-reference.json'

describe('frozen Browser data for actual Rust migration regression', () => {
  it('preserves the historical corpus and tolerances without executing an old engine', () => {
    expect(reference.contractVersion).toBe(1)
    expect(reference.cases.map(fixture => fixture.name)).toEqual(['neutral', 'exposure', 'white-balance', 'tone', 'curve'])
  })
  for (const fixture of reference.cases) {
    it(`keeps ${fixture.name} frozen reference complete for Native comparison`, () => {
      expect(fixture.sourceRgb8.length % 3).toBe(0)
      expect(fixture.browserRgb8).toHaveLength(fixture.sourceRgb8.length)
      expect([...fixture.sourceRgb8, ...fixture.browserRgb8].every(value => Number.isInteger(value) && value >= 0 && value <= 255)).toBe(true)
      expect(fixture.curve.every(point => Number.isFinite(point.x) && Number.isFinite(point.y))).toBe(true)
      expect(fixture.maxChannelDelta).toBeGreaterThanOrEqual(0)
      expect(fixture.maxMeanDelta).toBeGreaterThanOrEqual(0)
    })
  }
})
