import { describe, expect, it } from 'vitest'
import { commandCatalog, resolveCommandShortcut, searchCommands } from './commands'

const key = (value: string, options: Partial<KeyboardEvent> = {}) => ({
  key: value, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...options,
}) as KeyboardEvent

describe('production command architecture', () => {
  it('has unique commands and discovers commands by labels, keywords, and shortcuts', () => {
    expect(new Set(commandCatalog.map(({ id }) => id)).size).toBe(commandCatalog.length)
    expect(searchCommands('original').map(({ id }) => id)).toContain('before')
    expect(searchCommands('Ctrl/Cmd E').map(({ id }) => id)).toEqual(['export'])
    expect(searchCommands('retouch').map(({ id }) => id)).toEqual(['healing'])
  })

  it('maps Windows and macOS shortcuts to the same command ids', () => {
    expect(resolveCommandShortcut(key('z', { ctrlKey: true }))).toBe('undo')
    expect(resolveCommandShortcut(key('z', { metaKey: true, shiftKey: true }))).toBe('redo')
    expect(resolveCommandShortcut(key('c', { ctrlKey: true, shiftKey: true }))).toBe('copySettings')
    expect(resolveCommandShortcut(key('e', { metaKey: true }))).toBe('export')
    expect(resolveCommandShortcut(key('f', { shiftKey: true }))).toBe('filmstrip')
    expect(resolveCommandShortcut(key('m', { altKey: true }))).toBeNull()
  })
})
