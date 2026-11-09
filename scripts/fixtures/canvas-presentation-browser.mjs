// Isolated Chromium diagnostic; uses a real Native SRP4 frame and production
// parser/decoder/presentation. No private photos or native image processing here.
import { parseNativePreviewFrame } from '../../src/nativeRender'
import { decodeNativePreviewPayload, presentPreviewFrame } from '../../src/nativePreviewPresentation'

export async function verifyCanvasBackingReuse(frame) {
  const result = parseNativePreviewFrame(frame)
  if (result.payload.format !== 'rgba8') throw new Error('Actual Native RGBA frame required')
  const presentation = await decodeNativePreviewPayload(result)
  const times = { reset: [], reuse: [] }
  const canvases = {}
  for (const mode of Object.keys(times)) {
    const canvas = document.createElement('canvas')
    canvas.width = result.width
    canvas.height = result.height
    canvases[mode] = canvas
  }
  let mismatches = 0
  try {
    for (let round = 0; round < 22; round++) {
      for (const mode of round % 2 ? ['reset', 'reuse'] : ['reuse', 'reset']) {
        const canvas = canvases[mode]
        const context = canvas.getContext('2d')
        const started = performance.now()
        if (mode === 'reset') {
          canvas.width = result.width
          canvas.height = result.height
          context.imageSmoothingEnabled = true
          context.imageSmoothingQuality = 'high'
          context.drawImage(presentation.image, 0, 0)
        } else {
          presentPreviewFrame(context, presentation.image, result.width, result.height)
        }
        // Force completion equally for both modes. This diagnostic readback is
        // not part of production Native RGBA histogram/presentation.
        const actual = context.getImageData(0, 0, result.width, result.height).data
        if (round >= 2) times[mode].push(performance.now() - started)
        for (let index = 0; index < actual.length; index++) {
          if (actual[index] !== result.payload.bytes[index]) mismatches++
        }
      }
    }
  } finally {
    presentation.release()
  }
  if (mismatches) throw new Error('Canvas output differs from actual Native RGBA payload')
  const sample = document.createElement('canvas')
  sample.width = 2; sample.height = 1
  const sampleContext = sample.getContext('2d')
  sampleContext.fillStyle = 'lime'; sampleContext.fillRect(0, 0, 1, 1)
  const target = document.createElement('canvas')
  target.width = 2; target.height = 1
  const context = target.getContext('2d')
  context.fillStyle = 'red'; context.fillRect(0, 0, 2, 1)
  presentPreviewFrame(context, sample, 2, 1)
  const transparent = Array.from(context.getImageData(0, 0, 2, 1).data)
  if (transparent.join() !== '0,255,0,255,0,0,0,0') throw new Error('Old frame leaked through alpha')
  const stats = (values) => {
    const sorted = [...values].sort((a, b) => a - b)
    return { medianMs: (sorted[9] + sorted[10]) / 2, p95Ms: sorted[18], samplesMs: values }
  }
  return { scope: 'Native SRP4 parser/decoder + Chromium draw/readback; excludes IPC, input, OS presentation',
    width: result.width, height: result.height, comparisons: 44, mismatches,
    reset: stats(times.reset), reuse: stats(times.reuse),
    transparent, userAgent: navigator.userAgent }
}
