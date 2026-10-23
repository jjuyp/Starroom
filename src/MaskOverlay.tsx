import { useRef, useState, type PointerEvent, type KeyboardEvent } from 'react'
import type { RadialMask } from './previewPresentation'
import type { NativeMaskDefinition } from './nativeRender'
import { clientPointToNormalized } from './viewportCoordinates'
import { validLinearGeometry } from './maskWorkspace'
import { maskControlRadii, nudgeRadialMask, type MaskHandle } from './maskOverlayGeometry'

type Bounds = { left: number; top: number; width: number; height: number }

function ControlPoint({ bounds, x, y, label, kind, onPointerDown, onKeyDown }: {
  bounds: Bounds; x: number; y: number; label: string; kind: string
  onPointerDown: (event: PointerEvent<SVGGElement>) => void; onKeyDown: (event: KeyboardEvent<SVGGElement>) => void
}) {
  return <g className={`mask-control ${kind}`} role="button" tabIndex={0} aria-label={label}
    onPointerDown={onPointerDown} onKeyDown={onKeyDown}>
    <title>{label}；方向鍵微調，Shift 加快</title>
    <ellipse className="mask-control-hit" cx={x * 1000} cy={y * 1000} {...maskControlRadii(bounds.width, bounds.height, 12)} />
    <ellipse className="mask-control-pin" cx={x * 1000} cy={y * 1000} {...maskControlRadii(bounds.width, bounds.height, 4)} />
  </g>
}

export function MaskOverlay({ bounds, mask, feather = .2, onBeginEdit, onChange }: {
  bounds: Bounds; mask: RadialMask; feather?: number; onBeginEdit: () => void; onChange: (mask: RadialMask) => void
}) {
  const [dragMode, setDragMode] = useState<MaskHandle | null>(null)
  const moveOffset = useRef({ x: 0, y: 0 })
  const svgRef = useRef<SVGSVGElement>(null)
  const angle = mask.rotation * Math.PI / 180
  const rotatePoint = (localX: number, localY: number) => ({
    x: mask.x + localX * Math.cos(angle) - localY * Math.sin(angle),
    y: mask.y + localX * Math.sin(angle) + localY * Math.cos(angle),
  })
  const widthHandle = rotatePoint(mask.width / 2, 0)
  const heightHandle = rotatePoint(0, mask.height / 2)
  const rotationHandle = rotatePoint(0, -mask.height / 2 - .08)
  const beginDrag = (event: PointerEvent, mode: MaskHandle) => {
    if (event.button !== 0) return
    event.stopPropagation()
    onBeginEdit()
    const point = clientPointToNormalized(event, svgRef.current!.getBoundingClientRect())
    moveOffset.current = { x: point.x - mask.x, y: point.y - mask.y }
    setDragMode(mode)
    svgRef.current?.setPointerCapture(event.pointerId)
  }
  const nudge = (mode: MaskHandle) => (event: KeyboardEvent) => {
    const next = nudgeRadialMask(mask, mode, event.key, event.shiftKey)
    if (!next) return
    event.preventDefault(); event.stopPropagation(); onBeginEdit(); onChange(next)
  }
  return <svg ref={svgRef} className="mask-overlay" style={{ left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height }}
    viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-label="編輯放射漸層"
    onPointerDown={(event) => {
      if (event.button !== 0 || event.target !== event.currentTarget) return
      const point = clientPointToNormalized(event, event.currentTarget.getBoundingClientRect())
      onBeginEdit(); onChange({ ...mask, x: Math.max(0, Math.min(1, point.x)), y: Math.max(0, Math.min(1, point.y)) })
    }} onPointerMove={(event) => {
      if (!dragMode) return
      const next = clientPointToNormalized(event, event.currentTarget.getBoundingClientRect())
      const dx = next.x - mask.x, dy = next.y - mask.y
      const localX = dx * Math.cos(-angle) - dy * Math.sin(-angle)
      const localY = dx * Math.sin(-angle) + dy * Math.cos(-angle)
      if (dragMode === 'move') onChange({ ...mask, x: Math.max(0, Math.min(1, next.x - moveOffset.current.x)), y: Math.max(0, Math.min(1, next.y - moveOffset.current.y)) })
      if (dragMode === 'width') onChange({ ...mask, width: Math.max(.04, Math.min(1.6, Math.abs(localX) * 2)) })
      if (dragMode === 'height') onChange({ ...mask, height: Math.max(.04, Math.min(1.6, Math.abs(localY) * 2)) })
      if (dragMode === 'rotate') onChange({ ...mask, rotation: Math.atan2(dy, dx) * 180 / Math.PI + 90 })
    }} onPointerCancel={() => setDragMode(null)} onPointerUp={(event) => {
      setDragMode(null); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
    }}>
    <g transform={`rotate(${mask.rotation} ${mask.x * 1000} ${mask.y * 1000})`}>
      <ellipse className="mask-feather-ring" cx={mask.x * 1000} cy={mask.y * 1000} rx={mask.width * 500 * (1 + 2 * feather)} ry={mask.height * 500 * (1 + 2 * feather)} />
      <ellipse className="mask-ring-hit" cx={mask.x * 1000} cy={mask.y * 1000} rx={mask.width * 500} ry={mask.height * 500} onPointerDown={(event) => beginDrag(event, 'move')} />
      <ellipse className="mask-ring" cx={mask.x * 1000} cy={mask.y * 1000} rx={mask.width * 500} ry={mask.height * 500} />
    </g>
    <line className="mask-rotation-line" x1={mask.x * 1000} y1={mask.y * 1000} x2={rotationHandle.x * 1000} y2={rotationHandle.y * 1000} />
    <ControlPoint bounds={bounds} x={mask.x} y={mask.y} kind="mask-center-handle" label="移動遮罩" onPointerDown={(event) => beginDrag(event, 'move')} onKeyDown={nudge('move')} />
    <ControlPoint bounds={bounds} {...widthHandle} kind="mask-handle" label="調整遮罩寬度" onPointerDown={(event) => beginDrag(event, 'width')} onKeyDown={nudge('width')} />
    <ControlPoint bounds={bounds} {...heightHandle} kind="mask-handle" label="調整遮罩高度" onPointerDown={(event) => beginDrag(event, 'height')} onKeyDown={nudge('height')} />
    <ControlPoint bounds={bounds} {...rotationHandle} kind="mask-rotate-handle" label="旋轉遮罩" onPointerDown={(event) => beginDrag(event, 'rotate')} onKeyDown={nudge('rotate')} />
  </svg>
}

export function LinearMaskOverlay({ bounds, mask, onBeginEdit, onChange }: {
  bounds: Bounds; mask: Extract<NativeMaskDefinition, { type: 'linear' }>
  onBeginEdit: () => void; onChange: (mask: NativeMaskDefinition) => void
}) {
  const [drag, setDrag] = useState<'start' | 'end' | null>(null)
  const updatePoint = (point: 'start' | 'end', x: number, y: number) => {
    const next = point === 'start' ? { ...mask, startX: x, startY: y } : { ...mask, endX: x, endY: y }
    if (validLinearGeometry(next)) onChange(next)
  }
  return <svg className="mask-overlay linear-mask-overlay" aria-label="編輯線性漸層" style={{ left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height }}
    viewBox="0 0 1000 1000" preserveAspectRatio="none" onPointerMove={(event) => {
      if (!drag) return
      const point = clientPointToNormalized(event, event.currentTarget.getBoundingClientRect())
      updatePoint(drag, Math.max(0, Math.min(1, point.x)), Math.max(0, Math.min(1, point.y)))
    }} onPointerCancel={() => setDrag(null)} onPointerUp={(event) => {
      setDrag(null); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
    }}>
    <line className="mask-rotation-line" x1={mask.startX * 1000} y1={mask.startY * 1000} x2={mask.endX * 1000} y2={mask.endY * 1000} />
    {(['start', 'end'] as const).map((point) => <ControlPoint key={point} bounds={bounds} x={point === 'start' ? mask.startX : mask.endX}
      y={point === 'start' ? mask.startY : mask.endY} kind="mask-handle" label={point === 'start' ? '移動漸層起點' : '移動漸層終點'} onPointerDown={(event) => {
        if (event.button !== 0) return
        event.stopPropagation(); onBeginEdit(); setDrag(point)
        event.currentTarget.ownerSVGElement?.setPointerCapture(event.pointerId)
      }} onKeyDown={(event) => {
        if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return
        event.preventDefault(); event.stopPropagation(); onBeginEdit()
        const step = event.shiftKey ? .025 : .005
        const x = point === 'start' ? mask.startX : mask.endX, y = point === 'start' ? mask.startY : mask.endY
        updatePoint(point, Math.max(0, Math.min(1, x + (event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0))),
          Math.max(0, Math.min(1, y + (event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0))))
      }} />)}
  </svg>
}
