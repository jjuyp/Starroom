import type { RadialMask } from './previewPresentation'

export type MaskHandle = 'move' | 'width' | 'height' | 'rotate'

// UI geometry only: keep screen-space controls stable across zoom and aspect ratios.
export function maskControlRadii(width: number, height: number, radius: number) {
  return { rx: radius * 1000 / Math.max(1, width), ry: radius * 1000 / Math.max(1, height) }
}

export function nudgeRadialMask(mask: RadialMask, handle: MaskHandle, key: string, coarse = false): RadialMask | null {
  if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(key)) return null
  const direction = key === 'ArrowLeft' || key === 'ArrowUp' ? -1 : 1
  const step = coarse ? .025 : .005
  const bounded = (value: number) => Math.max(0, Math.min(1, value))
  if (handle === 'move') return key === 'ArrowLeft' || key === 'ArrowRight'
    ? { ...mask, x: bounded(mask.x + direction * step) }
    : { ...mask, y: bounded(mask.y + direction * step) }
  if (handle === 'rotate') return { ...mask, rotation: mask.rotation + direction * (coarse ? 5 : 1) }
  return { ...mask, [handle]: Math.max(.04, Math.min(1.6, mask[handle] + direction * step)) }
}
