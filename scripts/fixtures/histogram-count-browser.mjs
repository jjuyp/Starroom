import { parseNativePreviewFrame } from '../../src/nativeRender'
import { calculateDisplayHistogram, calculateHistogram } from '../../src/previewPresentation'

// Frozen prior display-byte counter, not a processing/color engine.
function previousHistogram(pixels, bins = 128) {
  const channels = [0, 1, 2].map(() => Array.from({ length: bins }, () => 0))
  const stride = Math.max(4, Math.floor(pixels.data.length / 300_000 / 4) * 4)
  for (let index = 0; index < pixels.data.length; index += stride) {
    if (pixels.data[index + 3] === 0) continue
    channels.forEach((channel, component) => {
      channel[Math.min(bins - 1, Math.floor(pixels.data[index + component] * bins / 256))]++
    })
  }
  const maximum = Math.max(1, ...channels.flat())
  const [red, green, blue] = channels.map((channel) => channel.map((value) => value / maximum))
  return { red, green, blue, luminance: calculateHistogram(pixels, bins) }
}

export function verifyHistogramCounting(frame) {
  const parsed = parseNativePreviewFrame(frame)
  if (parsed.payload.format !== 'rgba8') throw new Error('Actual Native RGBA frame required')
  const bytes = parsed.payload.bytes
  const data = new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const pixels = { data, width: parsed.width, height: parsed.height, colorSpace: 'srgb' }
  const samples = { previous: [], current: [] }
  let comparisons = 0
  for (let round = 0; round < 32; round++) {
    const results = {}
    for (const mode of round % 2 ? ['previous', 'current'] : ['current', 'previous']) {
      const started = performance.now()
      results[mode] = (mode === 'previous' ? previousHistogram : calculateDisplayHistogram)(pixels)
      if (round >= 2) samples[mode].push(performance.now() - started)
    }
    if (JSON.stringify(results.previous) !== JSON.stringify(results.current)) {
      throw new Error('Display histogram changed')
    }
    comparisons++
  }
  const stats = (values) => {
    const sorted = [...values].sort((a, b) => a - b)
    return { medianMs: (sorted[14] + sorted[15]) / 2, p95Ms: sorted[28], samplesMs: values }
  }
  return { scope: 'Display histogram CPU counting only, real Native RGBA payload; not input-to-display',
    width: parsed.width, height: parsed.height, comparisons, previous: stats(samples.previous),
    current: stats(samples.current), userAgent: navigator.userAgent }
}
