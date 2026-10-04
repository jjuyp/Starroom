import type { NativeLibraryAsset, NativeLibraryCollection, NativeLibraryQuery, NativeSessionState } from './nativeRender'

export type LibraryFilter = 'all' | 'recent' | 'five-star' | 'edited'

export function sessionLibraryQuery(state: NativeSessionState, collections: NativeLibraryCollection[]) {
  const filter: LibraryFilter = ['all', 'recent', 'five-star', 'edited'].includes(state.libraryContext)
    ? state.libraryContext as LibraryFilter : 'all'
  const browser = state.libraryBrowser ?? { collectionId: null, search: '', page: 0 }
  if (!Number.isSafeInteger(browser.page) || browser.page < 0 || browser.page > Math.floor(4294967295 / 200)) throw new Error('SessionInvalid: invalid Library page')
  const collection = browser.collectionId == null ? null : collections.find((item) => item.id === browser.collectionId)
  if (collection === undefined) throw new Error('SessionInvalid: 原先的收藏集已不存在，請捨棄工作階段復原後重新選取；照片與編輯不會刪除。')
  const query: NativeLibraryQuery = {
    collectionId: collection?.id ?? null, text: browser.search.trim() || null,
    limit: 200, offset: browser.page * 200, sort: 'importTime', direction: 'descending',
    recentBatch: filter === 'recent', minimumRating: filter === 'five-star' ? 5 : null, editedOnly: filter === 'edited',
  }
  return { filter, collection, search: browser.search, page: browser.page, query }
}

/** Restore compact catalog records; the selected editor asset may be outside the visible page. */
export async function loadSessionLibrary(state: NativeSessionState, collections: NativeLibraryCollection[],
  query: (query: NativeLibraryQuery) => Promise<NativeLibraryAsset[]>) {
  const scope = sessionLibraryQuery(state, collections)
  const assets = await query(scope.query)
  let selected = assets.find((asset) => asset.id === state.selectedAssetId || asset.sourcePath === state.selectedSourcePath)
  if (!selected && state.selectedAssetId !== null) {
    selected = (await query({ assetIds: [state.selectedAssetId], limit: 1 }))[0]
  }
  const editorAssets = selected && !assets.some((asset) => asset.id === selected.id) ? [...assets, selected] : assets
  return { ...scope, assets, editorAssets, selected }
}
