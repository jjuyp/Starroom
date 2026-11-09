import { defaultAdjustments, type Adjustments } from './editorState'

export interface ToneCurvePoint { id: string; x: number; y: number }
export interface RadialMask { x: number; y: number; width: number; height: number; rotation: number }
export interface DisplayPixels {
  data: Uint8ClampedArray
  width: number
  height: number
  colorSpace: PredefinedColorSpace
}

export function hasAdjustments(adjustments: Adjustments) {
  return (Object.keys(defaultAdjustments) as Array<keyof Adjustments>).some((key) => adjustments[key] !== defaultAdjustments[key])
}

/** Histogram analyzes only an already-rendered Native preview. */
export function calculateHistogram(imageData: DisplayPixels, bins = 48) {
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

export interface DisplayHistogram { luminance: number[]; red: number[]; green: number[]; blue: number[] }
/** Display-channel counts only: this never changes or color-transforms image pixels. */
export function calculateDisplayHistogram(imageData: DisplayPixels, bins = 128): DisplayHistogram {
  const redCounts = Array.from({ length: bins }, () => 0)
  const greenCounts = Array.from({ length: bins }, () => 0)
  const blueCounts = Array.from({ length: bins }, () => 0)
  const pixels = imageData.data
  const stride = Math.max(4, Math.floor(pixels.length / 300_000 / 4) * 4)
  // Avoid allocating/invoking a forEach closure for every sampled display pixel.
  // This counts already-rendered bytes only; no source or display color transform.
  for (let index = 0; index < pixels.length; index += stride) {
    if (pixels[index + 3] === 0) continue
    redCounts[Math.min(bins - 1, Math.floor(pixels[index] * bins / 256))] += 1
    greenCounts[Math.min(bins - 1, Math.floor(pixels[index + 1] * bins / 256))] += 1
    blueCounts[Math.min(bins - 1, Math.floor(pixels[index + 2] * bins / 256))] += 1
  }
  const maximum = Math.max(1, ...redCounts, ...greenCounts, ...blueCounts)
  const [red, green, blue] = [redCounts, greenCounts, blueCounts]
    .map((channel) => channel.map((value) => value / maximum))
  return { red, green, blue, luminance: calculateHistogram(imageData, bins) }
}
