import { describe, expect, it } from 'vitest'
import {
  MAX_INTERACTIVE_HISTORY,
  appendInteractiveHistory,
  prependInteractiveHistory,
  scrollFilmstripFromWheel,
} from './interactiveState'

describe('interactive history retention', () => {
  it('bounds browser fallback undo and redo state to 20', () => {
    const undo = Array.from({ length: 100 }, (_, index) => index)
      .reduce<number[]>((history, entry) => appendInteractiveHistory(history, entry), [])
    const redo = Array.from({ length: 100 }, (_, index) => index)
      .reduce<number[]>((history, entry) => prependInteractiveHistory(history, entry), [])
    expect(MAX_INTERACTIVE_HISTORY).toBe(20)
    expect(undo).toEqual(Array.from({ length: 20 }, (_, index) => index + 80))
    expect(redo).toEqual(Array.from({ length: 20 }, (_, index) => 99 - index))
  })
})

describe('filmstrip wheel navigation', () => {
  it('converts a vertical wheel gesture into horizontal movement only when overflowing', () => {
    const overflowing = { clientWidth: 400, scrollWidth: 2_000, scrollLeft: 50 }
    expect(scrollFilmstripFromWheel(overflowing, 0, 120)).toBe(true)
    expect(overflowing.scrollLeft).toBe(170)

    const fitting = { clientWidth: 400, scrollWidth: 400, scrollLeft: 0 }
    expect(scrollFilmstripFromWheel(fitting, 0, 120)).toBe(false)
    expect(fitting.scrollLeft).toBe(0)
  })

  it('leaves native horizontal touchpad gestures untouched', () => {
    const filmstrip = { clientWidth: 400, scrollWidth: 2_000, scrollLeft: 50 }
    expect(scrollFilmstripFromWheel(filmstrip, 80, 20)).toBe(false)
    expect(filmstrip.scrollLeft).toBe(50)
  })
})
