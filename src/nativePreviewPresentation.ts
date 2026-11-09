import type { NativePreviewResult } from './nativeRender'
import { calculateDisplayHistogram, type DisplayPixels } from './previewPresentation'

/** Both the fit and zoom canvases present whole frames. Setting either dimension,
 * even to its current value, discards the entire backing store and drawing state. */
export function presentPreviewFrame(context: CanvasRenderingContext2D, image: CanvasImageSource,
  width: number, height: number): void {
  const { canvas } = context
  if (canvas.width !== width) canvas.width = width
  if (canvas.height !== height) canvas.height = height
  context.imageSmoothingEnabled = true
  context.imageSmoothingQuality = 'high'
  // Replace rather than blend: a transparent pixel must never retain an older edit.
  context.globalCompositeOperation = 'copy'
  try {
    context.drawImage(image, 0, 0)
  } finally {
    context.globalCompositeOperation = 'source-over'
  }
}

/** Analyze already-transformed display bytes without a second full canvas readback.
 * JPEG retains the explicit decoded-canvas path; no pixel transform is performed here. */
export function nativePreviewDisplayHistogram(result: NativePreviewResult, readback: () => DisplayPixels) {
  const bytes = result.payload.bytes
  return calculateDisplayHistogram(result.payload.format === 'rgba8' ? {
    data: new Uint8ClampedArray(bytes.buffer as ArrayBuffer, bytes.byteOffset, bytes.byteLength),
    width: result.width, height: result.height, colorSpace: 'srgb',
  } : readback())
}

/** Presentation of already transformed sRGB pixels, never image-processing/color math. */
export async function decodeNativePreviewPayload(result: NativePreviewResult): Promise<{
  image: CanvasImageSource
  release: () => void
}> {
  const bytes = result.payload.bytes
  if (result.payload.format === 'rgba8') {
    const pixels = new Uint8ClampedArray(bytes.buffer as ArrayBuffer, bytes.byteOffset, bytes.byteLength)
    const bitmap = await createImageBitmap(new ImageData(pixels, result.width, result.height,
      { colorSpace: 'srgb' }), { colorSpaceConversion: 'none', premultiplyAlpha: 'none' })
    return { image: bitmap, release: () => bitmap.close() }
  }
  const jpegBuffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
  const blobUrl = URL.createObjectURL(new Blob([jpegBuffer], { type: 'image/jpeg' }))
  const release = () => URL.revokeObjectURL(blobUrl)
  const image = new Image()
  try {
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve()
      image.onerror = () => reject(new Error('Native preview JPEG could not be decoded.'))
      image.src = blobUrl
    })
    return { image, release }
  } catch (error) {
    release()
    throw error
  }
}
