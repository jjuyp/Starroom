import { describe, expect, it } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { createElement } from 'react'
import { maskControlRadii, nudgeRadialMask } from './maskOverlayGeometry'
import { MaskOverlay, LinearMaskOverlay } from './MaskOverlay'

describe('fine mask controls', () => {
  const mask = { x: .5, y: .5, width: .4, height: .5, rotation: 0 }
  it('keeps visible pins small but hit targets 24 CSS pixels at every zoom/aspect', () => {
    for (const [width, height] of [[100, 50], [800, 600], [1800, 3000]]) {
      for (const radius of [4, 12]) {
        const radii = maskControlRadii(width, height, radius)
        expect(radii.rx * width / 1000).toBeCloseTo(radius, 10)
        expect(radii.ry * height / 1000).toBeCloseTo(radius, 10)
      }
    }
    expect(maskControlRadii(0, 0, 4)).toEqual({ rx: 4000, ry: 4000 })
  })
  it('supports keyboard move, independent sizing and rotation without mutating state', () => {
    expect(nudgeRadialMask(mask, 'move', 'ArrowRight')).toMatchObject({ x: .505, y: .5 })
    expect(nudgeRadialMask(mask, 'move', 'ArrowUp', true)).toMatchObject({ x: .5, y: .475 })
    expect(nudgeRadialMask(mask, 'width', 'ArrowRight')).toMatchObject({ width: .405, height: .5 })
    expect(nudgeRadialMask(mask, 'height', 'ArrowDown', true)).toMatchObject({ width: .4, height: .525 })
    expect(nudgeRadialMask(mask, 'rotate', 'ArrowLeft', true)).toMatchObject({ rotation: -5 })
    expect(nudgeRadialMask(mask, 'move', 'Tab')).toBeNull()
    expect(mask).toEqual({ x: .5, y: .5, width: .4, height: .5, rotation: 0 })
  })
  it('keeps keyboard geometry within the same pointer/native contract limits', () => {
    expect(nudgeRadialMask({ ...mask, x: 1 }, 'move', 'ArrowRight')!.x).toBe(1)
    expect(nudgeRadialMask({ ...mask, y: 0 }, 'move', 'ArrowUp')!.y).toBe(0)
    expect(nudgeRadialMask({ ...mask, width: .04 }, 'width', 'ArrowLeft')!.width).toBe(.04)
    expect(nudgeRadialMask({ ...mask, height: 1.6 }, 'height', 'ArrowDown')!.height).toBe(1.6)
  })
  it('renders focusable named controls and invisible hit areas using the actual production components', () => {
    const bounds = { left: 0, top: 0, width: 800, height: 600 }
    const radial = renderToStaticMarkup(createElement(MaskOverlay, { bounds, mask, onBeginEdit() {}, onChange() {} }))
    expect(radial.match(/role="button"/g)).toHaveLength(4)
    expect(radial.match(/tabindex="0"/g)).toHaveLength(4)
    expect(radial.match(/class="mask-control-hit"/g)).toHaveLength(4)
    expect(radial).toContain('mask-ring-hit')
    expect(radial).toContain('aria-label="旋轉遮罩"')
    const linear = renderToStaticMarkup(createElement(LinearMaskOverlay, { bounds,
      mask: { type: 'linear', startX: .2, startY: .3, endX: .8, endY: .7, feather: .2, invert: false }, onBeginEdit() {}, onChange() {} }))
    expect(linear.match(/role="button"/g)).toHaveLength(2)
    expect(linear).toContain('aria-label="移動漸層終點"')
  })
})
