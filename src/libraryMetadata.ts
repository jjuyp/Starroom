// Catalog repair eligibility only; LibRaw remains authoritative for source dimensions.
export function needsRawMetadataRepair(metadata: { fileType: string; width: number | null; height: number | null }): boolean {
  return ['nef', 'arw', 'cr2', 'cr3', 'dng', 'raf'].includes(metadata.fileType.toLowerCase())
    && (metadata.width === null || metadata.height === null || metadata.width <= 32 || metadata.height <= 32)
}
