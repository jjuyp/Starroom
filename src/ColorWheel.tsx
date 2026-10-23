import { useRef, type CSSProperties, type PointerEvent } from 'react'
import { wheelKeyboardValue, wheelPosition, wheelValue, wrapWheelHue } from './editControls'

export function ColorWheel({ hue, chroma, label, onBeginEdit, onChange }: {
  hue: number; chroma: number; label: string; onBeginEdit: () => void
  onChange: (hue: number, chroma: number) => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const position = wheelPosition(hue, chroma)
  const update = (event: PointerEvent<HTMLDivElement>) => {
    const bounds = ref.current?.getBoundingClientRect()
    if (!bounds || bounds.width <= 0 || bounds.height <= 0) return
    const next = wheelValue((event.clientX - bounds.left) / bounds.width * 100,
      (event.clientY - bounds.top) / bounds.height * 100, hue)
    onChange(next.hue, next.chroma)
  }
  const release = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
  }
  return <div className="grading-wheel-shell"><div ref={ref} className="grading-wheel" role="slider" tabIndex={0}
    aria-label={`${label}色輪`} aria-valuemin={-180} aria-valuemax={180} aria-valuenow={Math.round(wrapWheelHue(hue))}
    aria-valuetext={`色相 ${Math.round(wrapWheelHue(hue))} 度，彩度 ${Math.round(chroma)}`}
    title="拖曳調整色相與彩度；方向鍵微調，Shift 加大幅度；Home 或雙擊回到中性色"
    onPointerDown={(event) => {
      if (event.button !== 0) return
      event.preventDefault(); event.currentTarget.focus(); onBeginEdit()
      event.currentTarget.setPointerCapture(event.pointerId); update(event)
    }}
    onPointerMove={(event) => { if (event.currentTarget.hasPointerCapture(event.pointerId)) update(event) }}
    onPointerUp={release} onPointerCancel={release}
    onDoubleClick={() => { onBeginEdit(); onChange(wrapWheelHue(hue), 0) }}
    onKeyDown={(event) => {
      const next = wheelKeyboardValue(hue, chroma, event.key, event.shiftKey)
      if (!next) return
      event.preventDefault(); onBeginEdit(); onChange(next.hue, next.chroma)
    }}>
    <span className="grading-wheel-center" /><span className="grading-wheel-handle" style={{ left: `${position.x}%`, top: `${position.y}%`, '--wheel-hue': wrapWheelHue(hue + (chroma < 0 ? 180 : 0)) } as CSSProperties} />
  </div><small>{Math.round(wrapWheelHue(hue))}° · C {Math.round(chroma)}</small></div>
}
