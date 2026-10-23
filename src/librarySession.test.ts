import { expect, it, vi } from 'vitest'
import { loadSessionLibrary, sessionLibraryQuery } from './librarySession'
import type { NativeLibraryAsset, NativeLibraryCollection, NativeSessionState } from './nativeRender'

const saved: NativeSessionState = {
  version: 1, workspace: 'edit', selectedAssetId: 700, selectedSourcePath: 'C:/photos/700.nef',
  activeTool: 'color', libraryPanelOpen: true, filmstripOpen: true, zoomMode: '100', zoomScale: 2,
  libraryContext: 'edited', libraryBrowser: { collectionId: 4, search: '京都', page: 2 },
}
const collections: NativeLibraryCollection[] = [{ id: 4, name: '旅行', kind: 'normal', rule: null }]
const asset = (id: number) => ({ id, sourcePath: `C:/photos/${id}.nef` }) as NativeLibraryAsset

it('restores collection, Unicode search, paging and edited filter before native query', () => {
  expect(sessionLibraryQuery(saved, collections)).toMatchObject({ filter: 'edited', page: 2, search: '京都', collection: collections[0],
    query: { collectionId: 4, text: '京都', offset: 400, limit: 200, editedOnly: true, sort: 'importTime' } })
})

it('migrates old filter-only sessions without inventing a collection or resetting the filter', () => {
  expect(sessionLibraryQuery({ ...saved, libraryBrowser: undefined, libraryContext: 'five-star' }, [])).toMatchObject({
    collection: null, page: 0, query: { minimumRating: 5, offset: 0, editedOnly: false },
  })
})

it('restores an off-page selected asset without adding it to the visible Library result', async () => {
  const query = vi.fn().mockResolvedValueOnce([asset(401)]).mockResolvedValueOnce([asset(700)])
  const result = await loadSessionLibrary(saved, collections, query)
  expect(query).toHaveBeenNthCalledWith(2, { assetIds: [700], limit: 1 })
  expect(result.assets.map((item) => item.id)).toEqual([401])
  expect(result.editorAssets.map((item) => item.id)).toEqual([401, 700])
  expect(result.selected?.id).toBe(700)
})

it('does not duplicate selection on the restored page or decode source pixels', async () => {
  const query = vi.fn().mockResolvedValue([asset(700)])
  const result = await loadSessionLibrary(saved, collections, query)
  expect(query).toHaveBeenCalledTimes(1)
  expect(result.editorAssets).toEqual(result.assets)
})

it('does not silently substitute All Photos for a deleted collection', async () => {
  const query = vi.fn()
  await expect(loadSessionLibrary(saved, [], query)).rejects.toThrow('SessionInvalid')
  expect(query).not.toHaveBeenCalled()
})

it('rejects invalid page state and propagates query failures to recovery UX', async () => {
  for (const page of [-1, .5, NaN, Infinity, 4294967296]) {
    expect(() => sessionLibraryQuery({ ...saved, libraryBrowser: { collectionId: null, search: '', page } }, [])).toThrow('SessionInvalid')
  }
  await expect(loadSessionLibrary(saved, collections, vi.fn().mockRejectedValue(new Error('DatabaseOpenFailed')))).rejects.toThrow('DatabaseOpenFailed')
})

it('reports missing selected assets without selecting a different photo as if restored', async () => {
  const result = await loadSessionLibrary(saved, collections, vi.fn().mockResolvedValue([]))
  expect(result.selected).toBeUndefined()
  expect(result.assets).toEqual([])
})
