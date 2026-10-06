export type ErrorCategory = 'File' | 'RAW' | 'Color' | 'Library' | 'Missing' | 'Relink' | 'AI' | 'Export' | 'Memory' | 'Permission' | 'Session' | 'Unknown'

export interface PresentedError { category: ErrorCategory; message: string; diagnostic: string }

const categoryNames: Record<ErrorCategory, string> = {
  File: '檔案', RAW: 'RAW', Color: '色彩', Library: '圖庫', Missing: '缺少檔案', Relink: '重新連結',
  AI: 'AI', Export: '匯出', Memory: '記憶體', Permission: '權限', Session: '工作階段', Unknown: '未知錯誤',
}

const categoryFor = (diagnostic: string): ErrorCategory => {
  const value = diagnostic.toLowerCase()
  if (value.includes('sourceoverwriteforbidden')) return 'Export'
  if (['masksourcemismatch', 'portraitsourcemismatch', 'portraitrestoremetadatamissing'].some((code) => value.includes(code))) return 'AI'
  if (value.includes('outofmemory') || value.includes('out of memory')) return 'Memory'
  if (value.includes('permission') || value.includes('access denied')) return 'Permission'
  if (value.includes('curvepreview') || value.includes('native_curve_preview') || value.includes('littlecms')) return 'Color'
  if (['whitebalance', 'white-balance', 'neutral-picker', 'chromatic adaptation', 'invalidwhitepoint'].some((term) => value.includes(term))) return 'Color'
  if (value.includes('raw') || value.includes('libraw') || value.includes('demosaic')) return 'RAW'
  if (value.includes('icc') || value.includes('profile') || value.includes('color')) return 'Color'
  if (value.includes('database') || value.includes('library') || value.includes('history')) return 'Library'
  if (value.includes('missing') || value.includes('not found')) return 'Missing'
  if (value.includes('relink') || value.includes('moved source')) return 'Relink'
  if (value.includes('model') || value.includes('portrait') || value.includes('mask') || value.includes('denoise')) return 'AI'
  if (value.includes('export') || value.includes('encode') || value.includes('destination')) return 'Export'
  if (value.includes('session') || value.includes('autosave') || value.includes('recovery')) return 'Session'
  if (value.includes('file') || value.includes('decode') || value.includes('source')) return 'File'
  return 'Unknown'
}

export function presentError(error: unknown, fallback: string): PresentedError {
  const diagnostic = error instanceof Error ? error.message : typeof error === 'string' ? error : fallback
  const category = categoryFor(diagnostic)
  const invalidNeutral = /invalidwhitebalancesample|invalidwhitepoint|neutral-picker sample is missing or invalid|chromatic adaptation requires/i.test(diagnostic)
  const message = diagnostic.includes('SourceOverwriteForbidden') ? '匯出不能覆寫原始照片。請更換檔名或匯出資料夾。'
    : /curvepreview|native_curve_preview/i.test(diagnostic) ? '無法顯示原生曲線，請重新開啟曲線面板，必要時復原最近的曲線調整。'
    : invalidNeutral ? '無法從這個區域取得有效白平衡。請選擇有亮度的中性灰或白色區域。'
    : diagnostic.includes('MaskSourceMismatch') ? '這個 AI 遮罩屬於另一張照片，請在目前照片重新產生遮罩。'
    : diagnostic.includes('PortraitSourceMismatch') ? '這個人像或肌膚選取屬於另一張照片，請重新偵測並選取人臉。'
      : diagnostic.includes('PortraitRestoreMetadataMissing') ? '這個舊人像選取缺少還原資料，請重新偵測並選取人臉。'
        : category === 'Memory' ? 'Starroom 沒有足夠的記憶體執行這項操作。'
    : category === 'Permission' ? 'Starroom 無法存取所選的檔案或資料夾。'
      : category === 'Missing' ? '缺少必要的來源檔案或本機模型。'
        : category === 'Session' ? 'Starroom 無法安全地還原或儲存這個工作階段。'
          : fallback
  return { category, message, diagnostic }
}

export function formatUserError(error: unknown, fallback: string) {
  const value = presentError(error, fallback)
  return `${categoryNames[value.category]}：${value.message} · 診斷資訊：${value.diagnostic}`
}
