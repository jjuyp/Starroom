/** Native reports photographic edit membership. This is an invalidation ledger, not an edit DB. */
export class CatalogEditMembership {
  private readonly values = new Map<number, boolean>()
  observe(assetId: number, edited: boolean | null | undefined): boolean {
    // Unknown/corrupt native state needs an explicit recount/error, never a fabricated zero.
    if (typeof edited !== 'boolean') return true
    const previous = this.values.get(assetId)
    this.values.set(assetId, edited)
    return previous === undefined ? edited : previous !== edited
  }
}
