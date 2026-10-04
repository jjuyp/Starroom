import { expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { queryNativeLibrary, queryNativeLibraryIds } from './nativeRender'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue([]), convertFileSrc: vi.fn(), isTauri: () => true }))

it('keeps edited state and collection scope on paginated query and full selection', async () => {
  const query = { collectionId: 42, editedOnly: true, text: 'Japan', offset: 200, limit: 200 }
  await queryNativeLibrary(query)
  expect(invoke).toHaveBeenLastCalledWith('library_query', expect.objectContaining({ editedOnly: true, query: expect.objectContaining({ collectionId: 42, text: 'Japan', offset: 200 }) }))
  await queryNativeLibraryIds(query)
  expect(invoke).toHaveBeenLastCalledWith('library_query_ids', expect.objectContaining({ editedOnly: true, query: expect.objectContaining({ collectionId: 42, text: 'Japan' }) }))
  await queryNativeLibrary()
  expect(invoke).toHaveBeenLastCalledWith('library_query', expect.objectContaining({ editedOnly: false }))
})
