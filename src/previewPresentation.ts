import { defaultAdjustments, type Adjustments } from './editorState'

export interface ToneCurvePoint { id: string; x: number; y: number }
export interface RadialMask { x: number; y: number; width: number; height: number; rotation: number }

const clamp01 = (value: number) => Math.min(1, Math.max(0, value))

/** UI-only curve visualization. Native Rust remains authoritative for pixel rendering. */
export function mapToneCurve(value: number, points?: ToneCurvePoint[]) {
  if (!points?.length) return value
  const sorted = [...points].filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y))
    .sort((a, b) => a.x - b.x).filter((point, index, array) => index === 0 || Math.abs(point.x - array[index - 1].x) >= 1e-6)
  if (sorted.length < 2) return value
  if (value <= sorted[0].x) return sorted[0].y
  if (value >= sorted.at(-1)!.x) return sorted.at(-1)!.y
  const slopes = sorted.slice(0, -1).map((point, index) => (sorted[index + 1].y - point.y) / Math.max(1e-6, sorted[index + 1].x - point.x))
  const tangents = sorted.map((_, index) => index === 0 ? slopes[0] : index === sorted.length - 1 ? slopes.at(-1)! : slopes[index - 1] * slopes[index] <= 0 ? 0 : (2 * slopes[index - 1] * slopes[index]) / (slopes[index - 1] + slopes[index]))
  const index = sorted.findIndex((point) => value <= point.x) - 1
  const left = sorted[index]; const right = sorted[index + 1]
  const width = Math.max(1e-6, right.x - left.x); const t = clamp01((value - left.x) / width); const t2 = t * t; const t3 = t2 * t
  return (2 * t3 - 3 * t2 + 1) * left.y + (t3 - 2 * t2 + t) * width * tangents[index]
    + (-2 * t3 + 3 * t2) * right.y + (t3 - t2) * width * tangents[index + 1]
}

export function hasAdjustments(adjustments: Adjustments) {
  return (Object.keys(defaultAdjustments) as Array<keyof Adjustments>).some((key) => adjustments[key] !== defaultAdjustments[key])
}

/** Histogram analyzes only an already-rendered Native preview. */
export function calculateHistogram(imageData: ImageData, bins = 48) {
  const values = Array.from({ length: bins }, () => 0)
  const pixels = imageData.data
  const stride = Math.max(4, Math.floor(pixels.length / 300_000 / 4) * 4)
  for (let index = 0; index < pixels.length; index += stride) {
    const luminance = .2126 * pixels[index] + .7152 * pixels[index + 1] + .0722 * pixels[index + 2]
    values[Math.min(bins - 1, Math.floor((luminance / 256) * bins))] += 1
  }
  const maximum = Math.max(...values, 1)
  return values.map((value) => value / maximum)
}
