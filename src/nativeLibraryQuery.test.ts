import { expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { queryNativeLibrary, queryNativeLibraryIds, queryNativeLibraryCounts } from './nativeRender'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue([]), convertFileSrc: vi.fn(), isTauri: () => true }))

it('loads whole-catalog counts without forwarding visible-page filters', async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ all: 100000, recent: 25, five: 16666, edited: 407 })
  expect(await queryNativeLibraryCounts()).toEqual({ all: 100000, recent: 25, five: 16666, edited: 407 })
  expect(invoke).toHaveBeenLastCalledWith('library_counts')
  vi.mocked(invoke).mockRejectedValueOnce(new Error('HistoryCorrupt'))
  await expect(queryNativeLibraryCounts()).rejects.toThrow('HistoryCorrupt')
})

it('keeps edited state and collection scope on paginated query and full selection', async () => {
  const query = { collectionId: 42, editedOnly: true, text: 'Japan', offset: 200, limit: 200 }
  await queryNativeLibrary(query)
  expect(invoke).toHaveBeenLastCalledWith('library_query', expect.objectContaining({ editedOnly: true, query: expect.objectContaining({ collectionId: 42, text: 'Japan', offset: 200 }) }))
  await queryNativeLibraryIds(query)
  expect(invoke).toHaveBeenLastCalledWith('library_query_ids', expect.objectContaining({ editedOnly: true, query: expect.objectContaining({ collectionId: 42, text: 'Japan' }) }))
  await queryNativeLibrary()
  expect(invoke).toHaveBeenLastCalledWith('library_query', expect.objectContaining({ editedOnly: false }))
})
