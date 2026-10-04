import { describe, expect, it } from 'vitest'
import { controlGradient, formatNumericValue, gradingGradient, relativeKelvin, wheelPosition, wheelValue, wheelKeyboardValue, whiteBalancePresentation } from './editControls'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { ColorWheel } from './ColorWheel'

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

  it('places negative chroma on the opposite side, matching the Native signed vector', () => {
    const negative = wheelPosition(45, -50)
    const opposite = wheelPosition(-135, 50)
    expect(negative.x).toBeCloseTo(opposite.x, 8)
    expect(negative.y).toBeCloseTo(opposite.y, 8)
  })

  it('preserves pointer direction when dragging outside the wheel', () => {
    const outside = wheelValue(250, 100)
    const inside = wheelValue(90, 60)
    expect(outside.hue).toBeCloseTo(inside.hue, 8)
    expect(outside.chroma).toBe(100)
  })

  it('keeps the chosen hue when resetting chroma at the center', () => {
    expect(wheelValue(50, 50, -123)).toEqual({ hue: -123, chroma: 0 })
    expect(wheelValue(Number.NaN, 50, 25)).toEqual({ hue: 25, chroma: 0 })
  })

  it('wraps keyboard hue and respects the same signed chroma range as numeric controls', () => {
    expect(wheelKeyboardValue(179, 30, 'ArrowRight')).toEqual({ hue: -179, chroma: 30 })
    expect(wheelKeyboardValue(-179, 30, 'ArrowLeft')).toEqual({ hue: 179, chroma: 30 })
    expect(wheelKeyboardValue(0, -50, 'ArrowDown')).toEqual({ hue: 0, chroma: -52 })
    expect(wheelKeyboardValue(0, -99, 'ArrowDown')).toEqual({ hue: 0, chroma: -100 })
    expect(wheelKeyboardValue(0, 99, 'ArrowUp')).toEqual({ hue: 0, chroma: 100 })
    expect(wheelKeyboardValue(0, 10, 'ArrowUp', true)).toEqual({ hue: 0, chroma: 20 })
    expect(wheelKeyboardValue(22, -50, 'Home')).toEqual({ hue: 22, chroma: 0 })
    expect(wheelKeyboardValue(22, 50, 'Tab')).toBeNull()
  })

  it('round-trips the visible vector for positive and negative saved chroma', () => {
    for (const hue of [-180, -135, -45, 0, 60, 179, 180]) {
      for (const chroma of [-100, -50, 0, 50, 100]) {
        const position = wheelPosition(hue, chroma)
        const canonical = wheelValue(position.x, position.y, hue)
        const result = wheelPosition(canonical.hue, canonical.chroma)
        expect(result.x).toBeCloseTo(position.x, 8)
        expect(result.y).toBeCloseTo(position.y, 8)
      }
    }
  })

  it('renders the actual wheel with accessible localized state and the signed handle position', () => {
    const markup = renderToStaticMarkup(createElement(ColorWheel, {
      hue: 0, chroma: -50, label: '陰影', onBeginEdit: () => {}, onChange: () => {},
    }))
    expect(markup).toContain('aria-label="陰影色輪"')
    expect(markup).toContain('彩度 -50')
    expect(markup).toContain('top:73%')
    expect(markup).toContain('role="slider"')
  })
})
