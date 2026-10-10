const nativePhotoExtensions = new Set(['jpg', 'jpeg', 'png', 'tif', 'tiff', 'nef', 'arw', 'cr2', 'cr3', 'dng', 'raf'])

export function supportedNativePhotoPaths(paths: readonly string[]) {
  return paths.filter((path) => nativePhotoExtensions.has(path.split('.').at(-1)?.toLowerCase() ?? ''))
}
