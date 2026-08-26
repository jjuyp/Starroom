import { describe, expect, it } from 'vitest'
import { clientPointToNormalized } from './viewportCoordinates'

describe('viewport coordinate mapping', () => {
  it('is invariant across equivalent CSS scaling and monitor DPI changes', () => {
    const at100 = clientPointToNormalized(
      { clientX: 500, clientY: 350 },
      { left: 100, top: 50, width: 800, height: 600 },
    )
    const at200 = clientPointToNormalized(
      { clientX: 1000, clientY: 700 },
      { left: 200, top: 100, width: 1600, height: 1200 },
    )
    expect(at100).toEqual({ x: 0.5, y: 0.5 })
    expect(at200).toEqual(at100)
  })

  it('supports curve coordinates and clamps out-of-bounds pointer input', () => {
    expect(clientPointToNormalized(
      { clientX: -10, clientY: 250 },
      { left: 0, top: 0, width: 100, height: 200 },
      true,
    )).toEqual({ x: 0, y: 0 })
  })

  it('does not emit NaN for an unmeasurable hidden surface', () => {
    expect(clientPointToNormalized(
      { clientX: 40, clientY: 50 },
      { left: 40, top: 50, width: 0, height: 0 },
    )).toEqual({ x: 0, y: 0 })
  })
})
