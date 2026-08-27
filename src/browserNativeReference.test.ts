import { describe, expect, it } from 'vitest'
import reference from '../tests/fixtures/m1c/browser-native-reference.json'
import { defaultAdjustments } from './editorState'
import { processImageData, type ToneCurvePoint } from './imagePipeline'

describe('frozen Browser reference for Native CPU migration', () => {
  for (const fixture of reference.cases) {
    it(`keeps ${fixture.name} Browser reference explicit`, () => {
      const rgba = fixture.sourceRgb8.flatMap((value, index) => index % 3 === 2 ? [value, 255] : [value])
      const source = { data: new Uint8ClampedArray(rgba), width: fixture.sourceRgb8.length / 3, height: 1 } as ImageData
      const curve = fixture.curve.map((point, index) => ({ id: `${fixture.name}-${index}`, ...point })) as ToneCurvePoint[]
      const output = processImageData(source, { ...defaultAdjustments, ...fixture.adjustments }, curve)
      const rgb = Array.from(output.data).filter((_, index) => index % 4 !== 3)
      expect(rgb).toEqual(fixture.browserRgb8)
    })
  }
})
