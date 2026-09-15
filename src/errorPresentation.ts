export type ErrorCategory = 'File' | 'RAW' | 'Color' | 'Library' | 'Missing' | 'Relink' | 'AI' | 'Export' | 'Memory' | 'Permission' | 'Session' | 'Unknown'

export interface PresentedError { category: ErrorCategory; message: string; diagnostic: string }

const categoryNames: Record<ErrorCategory, string> = {
  File: '檔案', RAW: 'RAW', Color: '色彩', Library: '圖庫', Missing: '缺少檔案', Relink: '重新連結',
  AI: 'AI', Export: '匯出', Memory: '記憶體', Permission: '權限', Session: '工作階段', Unknown: '未知錯誤',
}

const categoryFor = (diagnostic: string): ErrorCategory => {
  const value = diagnostic.toLowerCase()
  if (value.includes('outofmemory') || value.includes('out of memory')) return 'Memory'
  if (value.includes('permission') || value.includes('access denied')) return 'Permission'
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
  const message = category === 'Memory' ? 'Starroom 沒有足夠的記憶體執行這項操作。'
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
