export type ErrorCategory = 'File' | 'RAW' | 'Color' | 'Library' | 'Missing' | 'Relink' | 'AI' | 'Export' | 'Memory' | 'Permission' | 'Session' | 'Unknown'

export interface PresentedError { category: ErrorCategory; message: string; diagnostic: string }

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
  const message = category === 'Memory' ? 'Starroom does not have enough memory for this operation.'
    : category === 'Permission' ? 'Starroom cannot access the selected file or folder.'
      : category === 'Missing' ? 'A required source file or local model is missing.'
        : category === 'Session' ? 'Starroom could not safely restore or save this session.'
          : fallback
  return { category, message, diagnostic }
}

export function formatUserError(error: unknown, fallback: string) {
  const value = presentError(error, fallback)
  return `${value.category}: ${value.message} · ${value.diagnostic}`
}
