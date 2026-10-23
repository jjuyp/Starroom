export type CommandId = 'undo' | 'redo' | 'copySettings' | 'pasteSettings' | 'before' | 'mask' | 'healing' | 'crop'
  | 'rate0' | 'rate1' | 'rate2' | 'rate3' | 'rate4' | 'rate5' | 'pick' | 'reject' | 'fit' | 'oneToOne'
  | 'filmstrip' | 'panels' | 'export'

export interface StarroomCommand { id: CommandId; label: string; shortcut: string; keywords: string }

export const commandCatalog: StarroomCommand[] = [
  { id: 'undo', label: '復原', shortcut: 'Ctrl/Cmd Z', keywords: '歷史 history' },
  { id: 'redo', label: '重做', shortcut: 'Ctrl/Cmd Shift Z', keywords: '歷史 history' },
  { id: 'copySettings', label: '複製設定', shortcut: 'Ctrl/Cmd Shift C', keywords: '編輯 剪貼簿 edit clipboard' },
  { id: 'pasteSettings', label: '貼上設定', shortcut: 'Ctrl/Cmd Shift V', keywords: '編輯 剪貼簿 edit clipboard' },
  { id: 'before', label: '切換編輯前／後', shortcut: 'B', keywords: '比較 原圖 compare original' },
  { id: 'mask', label: '開啟遮罩', shortcut: 'M', keywords: '局部調整 local adjustment' },
  { id: 'healing', label: '開啟修復', shortcut: 'H', keywords: '修飾 移除 retouch remove' },
  { id: 'crop', label: '開啟裁切／幾何', shortcut: 'C', keywords: '旋轉 變形 rotate transform' },
  ...([0, 1, 2, 3, 4, 5] as const).map((rating) => ({ id: `rate${rating}` as CommandId, label: rating === 0 ? '清除評分' : `評為 ${rating} 星`, shortcut: String(rating), keywords: '評分 圖庫 rating library' })),
  { id: 'pick', label: '標記為保留', shortcut: 'P', keywords: '圖庫 標記 library flag' },
  { id: 'reject', label: '標記為拒絕', shortcut: 'X', keywords: '圖庫 標記 library flag' },
  { id: 'fit', label: '縮放至適合視窗', shortcut: 'F', keywords: '畫布 檢視 canvas view' },
  { id: 'oneToOne', label: '縮放至 1:1', shortcut: 'Z', keywords: '畫布 100 percent' },
  { id: 'filmstrip', label: '顯示／隱藏底片列', shortcut: 'Shift F', keywords: '面板 縮圖 panel thumbnails' },
  { id: 'panels', label: '顯示／隱藏圖庫面板', shortcut: '\\', keywords: '側邊欄 面板 sidebar panel' },
  { id: 'export', label: '匯出', shortcut: 'Ctrl/Cmd E', keywords: '輸出 render output' },
]

export function searchCommands(query: string) {
  const normalized = query.trim().toLowerCase()
  return commandCatalog.filter((command) => !normalized || `${command.label} ${command.keywords} ${command.shortcut}`.toLowerCase().includes(normalized))
}

export function resolveCommandShortcut(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>): CommandId | null {
  const modifier = event.ctrlKey || event.metaKey
  const key = event.key.toLowerCase()
  if (modifier && key === 'z') return event.shiftKey ? 'redo' : 'undo'
  if (modifier && key === 'y') return 'redo'
  if (modifier && event.shiftKey && key === 'c') return 'copySettings'
  if (modifier && event.shiftKey && key === 'v') return 'pasteSettings'
  if (modifier && key === 'e') return 'export'
  if (modifier || event.altKey) return null
  if (key === 'b') return 'before'
  if (key === 'm') return 'mask'
  if (key === 'h') return 'healing'
  if (key === 'c') return 'crop'
  if (/^[0-5]$/.test(key)) return `rate${key}` as CommandId
  if (key === 'p') return 'pick'
  if (key === 'x') return 'reject'
  if (key === 'f') return event.shiftKey ? 'filmstrip' : 'fit'
  if (key === 'z') return 'oneToOne'
  if (event.key === '\\') return 'panels'
  return null
}
