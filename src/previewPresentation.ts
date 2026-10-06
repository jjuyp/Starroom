import { defaultAdjustments, type Adjustments } from './editorState'

export interface ToneCurvePoint { id: string; x: number; y: number }
export interface RadialMask { x: number; y: number; width: number; height: number; rotation: number }

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

export interface DisplayHistogram { luminance: number[]; red: number[]; green: number[]; blue: number[] }
/** Display-channel counts only: this never changes or color-transforms image pixels. */
export function calculateDisplayHistogram(imageData: ImageData, bins = 128): DisplayHistogram {
  const channels = [0, 1, 2].map(() => Array.from({ length: bins }, () => 0))
  const stride = Math.max(4, Math.floor(imageData.data.length / 300_000 / 4) * 4)
  for (let index = 0; index < imageData.data.length; index += stride) {
    if (imageData.data[index + 3] === 0) continue
    channels.forEach((channel, component) => { channel[Math.min(bins - 1, Math.floor(imageData.data[index + component] * bins / 256))] += 1 })
  }
  const maximum = Math.max(1, ...channels.flat())
  const [red, green, blue] = channels.map((channel) => channel.map((value) => value / maximum))
  return { red, green, blue, luminance: calculateHistogram(imageData, bins) }
}
