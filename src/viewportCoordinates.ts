export interface ClientRectLike {
  left: number
  top: number
  width: number
  height: number
}

export interface ClientPoint {
  clientX: number
  clientY: number
}

export function clientPointToNormalized(
  point: ClientPoint,
  bounds: ClientRectLike,
  invertY = false,
) {
  if (!(bounds.width > 0) || !(bounds.height > 0)) return { x: 0, y: 0 }
  const x = Math.max(0, Math.min(1, (point.clientX - bounds.left) / bounds.width))
  const y = Math.max(0, Math.min(1, (point.clientY - bounds.top) / bounds.height))
  return { x, y: invertY ? 1 - y : y }
}
