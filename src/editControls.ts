export type MixerBand = 'Red' | 'Orange' | 'Yellow' | 'Green' | 'Cyan' | 'Blue' | 'Purple' | 'Magenta'

const bandHue: Record<MixerBand, number> = {
  Red: 18, Orange: 52, Yellow: 92, Green: 145,
  Cyan: 195, Blue: 250, Purple: 292, Magenta: 330,
}

export function controlGradient(label: string, band?: MixerBand) {
  const normalized = label.toLowerCase()
  if (normalized.includes('temperature') || normalized.includes('色溫')) return 'linear-gradient(90deg,#3c78e8 0%,#8bb7df 28%,#d5d8cf 50%,#efb75d 76%,#e8833f 100%)'
  if (normalized.includes('tint') || normalized.includes('色調')) return 'linear-gradient(90deg,#45a56d 0%,#86c28f 26%,#c6c7bd 50%,#ce83c6 76%,#b84dc2 100%)'
  if (normalized.includes('saturation') || normalized.includes('vibrance') || normalized.includes('飽和')) return 'linear-gradient(90deg,#747d88,#849ba1 30%,#6c89e9 62%,#d65fa9)'
  const selected = band ?? (Object.keys(bandHue) as MixerBand[]).find((name) => normalized.includes(name.toLowerCase()))
  if (!selected) return undefined
  const hue = bandHue[selected]
  if (normalized.includes('hue')) return `linear-gradient(90deg,hsl(${hue - 34} 72% 55%),hsl(${hue} 78% 57%),hsl(${hue + 34} 72% 55%))`
  if (normalized.includes('chroma')) return `linear-gradient(90deg,hsl(${hue} 8% 45%),hsl(${hue} 42% 52%),hsl(${hue} 92% 58%))`
  if (normalized.includes('lightness')) return `linear-gradient(90deg,hsl(${hue} 66% 13%),hsl(${hue} 70% 50%),hsl(${hue} 52% 88%))`
  return undefined
}

export function gradingGradient(property: 'Hue' | 'Chroma' | 'Lightness', hue: number) {
  const normalizedHue = ((hue % 360) + 360) % 360
  if (property === 'Hue') {
    return 'linear-gradient(90deg,#ef5967 0%,#f3b64f 17%,#d7dd53 33%,#4bc886 50%,#4cbfe2 67%,#7772ef 83%,#db63c4 100%)'
  }
  if (property === 'Chroma') {
    return `linear-gradient(90deg,hsl(${normalizedHue} 4% 48%),hsl(${normalizedHue} 42% 54%),hsl(${normalizedHue} 92% 58%))`
  }
  return `linear-gradient(90deg,hsl(${normalizedHue} 58% 10%),hsl(${normalizedHue} 68% 50%),hsl(${normalizedHue} 44% 90%))`
}

export function formatNumericValue(value: number, step: number) {
  const digits = step < .1 ? 2 : step < 1 ? 1 : 0
  const rounded = value.toFixed(digits)
  return value > 0 ? `+${rounded}` : rounded
}

export function relativeKelvin(baseKelvin: number, relative: number) {
  const baseMired = 1_000_000 / Math.min(25_000, Math.max(1_500, baseKelvin))
  const adjustedMired = baseMired - relative * 1.35
  return Math.round(Math.min(25_000, Math.max(1_500, 1_000_000 / adjustedMired)) / 50) * 50
}

export function kelvinToRelative(baseKelvin: number, kelvin: number) {
  const baseMired = 1_000_000 / Math.min(25_000, Math.max(1_500, baseKelvin))
  const targetMired = 1_000_000 / Math.min(25_000, Math.max(1_500, kelvin))
  return Math.min(100, Math.max(-100, (baseMired - targetMired) / 1.35))
}

export function whiteBalancePresentation(baseKelvin: number | null, relative: number, isRaw: boolean) {
  if (isRaw && baseKelvin !== null) return { value: relativeKelvin(baseKelvin, relative), unit: 'K', native: true }
  return { value: relative, unit: 'relative', native: false }
}

export function wheelPosition(hue: number, chroma: number) {
  const radians = (hue - 90) * Math.PI / 180
  // Signed radius mirrors the Native wheel's signed chroma vector. This is UI geometry,
  // not image processing; do not silently canonicalize the saved adjustment/history.
  const radius = Math.min(1, Math.max(-1, chroma / 100)) * 46
  return { x: 50 + Math.cos(radians) * radius, y: 50 + Math.sin(radians) * radius }
}

export function wrapWheelHue(hue: number) {
  return Number.isFinite(hue) ? ((hue + 180) % 360 + 360) % 360 - 180 : 0
}

export function wheelValue(x: number, y: number, neutralHue = 0) {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return { hue: wrapWheelHue(neutralHue), chroma: 0 }
  const dx = x - 50
  const dy = y - 50
  const distance = Math.hypot(dx, dy)
  // Clamp the radius, not each axis: pointer capture allows dragging outside the element.
  // At zero chroma preserve the chosen hue rather than inventing a 90-degree selection.
  return {
    hue: distance < 1e-8 ? wrapWheelHue(neutralHue) : wrapWheelHue(Math.atan2(dy, dx) * 180 / Math.PI + 90),
    chroma: Math.min(46, distance) / 46 * 100,
  }
}

export function wheelKeyboardValue(hue: number, chroma: number, key: string, coarse = false) {
  const step = coarse ? 10 : 2
  if (key === 'ArrowLeft') return { hue: wrapWheelHue(hue - step), chroma }
  if (key === 'ArrowRight') return { hue: wrapWheelHue(hue + step), chroma }
  if (key === 'ArrowUp') return { hue: wrapWheelHue(hue), chroma: Math.min(100, chroma + step) }
  if (key === 'ArrowDown') return { hue: wrapWheelHue(hue), chroma: Math.max(-100, chroma - step) }
  if (key === 'Home') return { hue: wrapWheelHue(hue), chroma: 0 }
  return null
}
