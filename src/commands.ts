export type CommandId = 'undo' | 'redo' | 'copySettings' | 'pasteSettings' | 'before' | 'mask' | 'healing' | 'crop'
  | 'rate1' | 'rate2' | 'rate3' | 'rate4' | 'rate5' | 'pick' | 'reject' | 'fit' | 'oneToOne'
  | 'filmstrip' | 'panels' | 'export'

export interface StarroomCommand { id: CommandId; label: string; shortcut: string; keywords: string }

export const commandCatalog: StarroomCommand[] = [
  { id: 'undo', label: 'Undo', shortcut: 'Ctrl/Cmd Z', keywords: 'history' },
  { id: 'redo', label: 'Redo', shortcut: 'Ctrl/Cmd Shift Z', keywords: 'history' },
  { id: 'copySettings', label: 'Copy settings', shortcut: 'Ctrl/Cmd Shift C', keywords: 'edit clipboard' },
  { id: 'pasteSettings', label: 'Paste settings', shortcut: 'Ctrl/Cmd Shift V', keywords: 'edit clipboard' },
  { id: 'before', label: 'Toggle Before/After', shortcut: 'B', keywords: 'compare original' },
  { id: 'mask', label: 'Open Masks', shortcut: 'M', keywords: 'local adjustment' },
  { id: 'healing', label: 'Open Healing', shortcut: 'H', keywords: 'retouch remove' },
  { id: 'crop', label: 'Open Crop / Geometry', shortcut: 'C', keywords: 'rotate transform' },
  ...([1, 2, 3, 4, 5] as const).map((rating) => ({ id: `rate${rating}` as CommandId, label: `Rate ${rating} star${rating === 1 ? '' : 's'}`, shortcut: String(rating), keywords: 'rating library' })),
  { id: 'pick', label: 'Flag as Pick', shortcut: 'P', keywords: 'library flag' },
  { id: 'reject', label: 'Flag as Reject', shortcut: 'X', keywords: 'library flag' },
  { id: 'fit', label: 'Zoom to Fit', shortcut: 'F', keywords: 'canvas view' },
  { id: 'oneToOne', label: 'Zoom 1:1', shortcut: 'Z', keywords: 'canvas 100 percent' },
  { id: 'filmstrip', label: 'Toggle Filmstrip', shortcut: 'Shift F', keywords: 'panel thumbnails' },
  { id: 'panels', label: 'Toggle Library Panel', shortcut: '\\', keywords: 'sidebar panel' },
  { id: 'export', label: 'Export', shortcut: 'Ctrl/Cmd E', keywords: 'render output' },
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
  if (/^[1-5]$/.test(key)) return `rate${key}` as CommandId
  if (key === 'p') return 'pick'
  if (key === 'x') return 'reject'
  if (key === 'f') return event.shiftKey ? 'filmstrip' : 'fit'
  if (key === 'z') return 'oneToOne'
  if (event.key === '\\') return 'panels'
  return null
}
