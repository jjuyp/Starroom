import { describe, expect, it } from 'vitest'
import { controlGradient, formatNumericValue, gradingGradient, relativeKelvin, wheelPosition, wheelValue, whiteBalancePresentation } from './editControls'

describe('Starroom V2 editing controls', () => {
  it('keeps rendered-image white balance explicitly relative', () => {
    expect(whiteBalancePresentation(null, 12, false)).toEqual({ value: 12, unit: 'relative', native: false })
  })

  it('maps RAW relative correction around a real as-shot Kelvin baseline', () => {
    expect(relativeKelvin(5200, 0)).toBe(5200)
    expect(relativeKelvin(5200, 20)).toBeGreaterThan(5200)
    expect(whiteBalancePresentation(5200, 0, true)).toEqual({ value: 5200, unit: 'K', native: true })
  })

  it('uses semantic color gradients for temperature, tint and selected mixer target', () => {
    expect(controlGradient('Temperature')).toContain('#3c78e8')
    expect(controlGradient('Tint')).toContain('#45a56d')
    expect(controlGradient('Blue Hue', 'Blue')).toContain('hsl(250')
    expect(controlGradient('Red Chroma', 'Red')).toContain('92%')
    expect(gradingGradient('Hue', 0)).toContain('#4cbfe2')
    expect(gradingGradient('Chroma', -110)).toContain('hsl(250')
    expect(gradingGradient('Lightness', 30)).toContain('90%')
  })

  it('formats signed precision values without clipping intent', () => {
    expect(formatNumericValue(.35, .01)).toBe('+0.35')
    expect(formatNumericValue(-2, 1)).toBe('-2')
  })

  it('round-trips grading wheel state', () => {
    const position = wheelPosition(45, 50)
    const value = wheelValue(position.x, position.y)
    expect(value.hue).toBeCloseTo(45, 4)
    expect(value.chroma).toBeCloseTo(50, 4)
  })
})
