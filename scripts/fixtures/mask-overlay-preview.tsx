import { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { Circle, ScanLine, SlidersHorizontal } from 'lucide-react'
import { MaskOverlay, LinearMaskOverlay } from '../../src/MaskOverlay'
import type { RadialMask } from '../../src/previewPresentation'
import type { NativeMaskDefinition } from '../../src/nativeRender'
import '../../src/styles.css'
import '../../src/glass.css'
import './mask-overlay-preview.css'

// Visual/interaction fixture for the actual production overlay. This page never
// impersonates Tauri, Native rendering, AI availability or an installed product.
export function Review() {
  const [mode, setMode] = useState<'radial' | 'linear'>('radial')
  const [mask, setMask] = useState<RadialMask>({ x: .5, y: .51, width: .52, height: .56, rotation: 0 })
  const [linear, setLinear] = useState<Extract<NativeMaskDefinition, { type: 'linear' }>>({ type: 'linear', startX: .3, startY: .25, endX: .7, endY: .7, feather: .3, invert: false })
  const bounds = { left: 0, top: 0, width: 650, height: 650 }
  return <main className="app theme-dark mask-review">
    <header><span className="review-brand">✦ Starroom</span><div><strong>遮罩・細框設計</strong><small>實際 production 元件預覽 · 不是 Native 影像／安裝驗收</small></div><span className="review-tag">UI REVIEW</span></header>
    <div className="review-grid"><section className="review-photo"><div className="review-photo-heading"><strong>局部調整</strong><span>8 px 控制點 · 24 px 操作範圍</span></div>
      <div className="review-image"><img src="/fixtures/golden/sources/astronaut-eileen-collins.png" alt="合法 NASA 真人測試素材" />
        {mode === 'radial' ? <MaskOverlay bounds={bounds} mask={mask} feather={.12} onBeginEdit={() => {}} onChange={setMask} />
          : <LinearMaskOverlay bounds={bounds} mask={linear} onBeginEdit={() => {}} onChange={(value) => { if (value.type === 'linear') setLinear(value) }} />}
      </div><footer>細虛線不遮照片 · 拖曳控制點 · Tab／方向鍵可微調</footer></section>
      <aside className="mask-workspace"><div className="mask-workspace-title"><SlidersHorizontal size={20} /><div><strong>遮罩工具</strong><small>少量視覺、精準操作</small></div></div>
        <div className="review-mode"><button aria-pressed={mode === 'radial'} onClick={() => setMode('radial')}><Circle size={18} />放射漸層</button>
          <button aria-pressed={mode === 'linear'} onClick={() => setMode('linear')}><ScanLine size={18} />線性漸層</button></div>
        <div className="review-note"><strong>框線 1.1 px</strong><p>取消粗框與大片填色，留下低調的紫白細線。羽化範圍用較淡的 0.75 px 導引線表示。</p></div>
        <div className="review-note"><strong>小控制點、大操作範圍</strong><p>看得見的圓點只有 8 px；透明操作範圍有 24 px，不需要瞄準細線。</p></div>
        <div className="review-note"><strong>鍵盤可用</strong><p>Tab 選擇控制點，方向鍵微調位置、尺寸與旋轉；Shift 可加快。</p></div>
        <div className="review-values"><span>中心</span><output>{mask.x.toFixed(3)} / {mask.y.toFixed(3)}</output><span>尺寸</span><output>{mask.width.toFixed(3)} / {mask.height.toFixed(3)}</output><span>旋轉</span><output>{mask.rotation.toFixed(1)}°</output></div>
      </aside></div>
  </main>
}
createRoot(document.getElementById('root')!).render(<Review />)
